use crate::data::assistant::{self, Assistant, Message};
use crate::data::Error;

use futures::{SinkExt, Stream, StreamExt};

pub fn send(
    assistant: &Assistant,
    history: &[assistant::Message],
    message: &str,
) -> impl Stream<Item = Result<Event, Error>> {
    const SYSTEM_PROMPT: &str = "You are a helpful assistant.";

    let assistant = assistant.clone();
    let mut messages = history.to_vec();
    let message = message.to_owned();

    iced::stream::try_channel(1, |mut sender| async move {
        messages.push(Message::User(message.clone()));

        let _ = sender
            .send(Event::MessageSent(Message::User(message)))
            .await;

        let _ = sender
            .send(Event::MessageAdded(Message::Assistant(String::new())))
            .await;

        let mut message = String::new();

        {
            let mut next_message = assistant.complete(SYSTEM_PROMPT, &messages).boxed();
            let mut first = false;

            while let Some(token) = next_message.next().await.transpose()? {
                message.push_str(&token);

                let event = if first {
                    first = false;
                    Event::MessageAdded
                } else {
                    Event::LastMessageChanged
                };

                let _ = sender
                    .send(event(Message::Assistant(message.trim().to_owned())))
                    .await;
            }
        }

        // Suggest a title after the 1st and 5th messages
        if messages.len() == 1 || messages.len() == 5 {
            messages.push(Message::Assistant(message.trim().to_owned()));
            messages.push(Message::User(
                "Give me a short title for our conversation so far. \
                    Just the title between quotes; don't say anything else."
                    .to_owned(),
            ));

            let mut title_suggestion = assistant.complete(SYSTEM_PROMPT, &messages).boxed();
            let mut title = String::new();

            while let Some(token) = title_suggestion.next().await.transpose()? {
                title.push_str(&token);

                if title.len() > 80 {
                    title.push_str("...");
                }

                let _ = sender
                    .send(Event::TitleChanged(
                        title.trim().trim_matches('"').to_owned(),
                    ))
                    .await;

                if title.len() > 80 {
                    break;
                }
            }
        }

        let _ = sender.send(Event::ExchangeOver).await;

        Ok(())
    })
}

#[derive(Debug, Clone)]
pub enum Event {
    MessageSent(Message),
    MessageAdded(Message),
    LastMessageChanged(Message),
    ExchangeOver,
    TitleChanged(String),
}
