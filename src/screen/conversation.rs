use crate::assistant::{self, Assistant, ChatError, ChatEvent};

use iced::widget::{
    self, center, column, container, scrollable, text, text_input,
};
use iced::{Alignment, Border, Element, Font, Length, Padding, Task, Theme};

pub struct Conversation {
    assistant: Assistant,
    history: Vec<assistant::Message>,
    input: String,
    error: Option<ChatError>,
    is_sending: bool,
}

#[derive(Debug, Clone)]
pub enum Message {
    InputChanged(String),
    InputSubmitted,
    Chatting(Result<ChatEvent, ChatError>),
}

impl Conversation {
    pub fn new(assistant: Assistant) -> (Self, Task<Message>) {
        (
            Self {
                assistant,
                history: Vec::new(),
                input: String::new(),
                error: None,
                is_sending: false,
            },
            widget::focus_next(),
        )
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::InputChanged(input) => {
                self.input = input;
                self.error = None;

                Task::none()
            }
            Message::InputSubmitted => {
                self.is_sending = true;

                Task::run(
                    self.assistant.chat(&self.history, &self.input),
                    Message::Chatting,
                )
            }
            Message::Chatting(Ok(event)) => {
                match event {
                    ChatEvent::MessageSent(message) => {
                        self.history.push(message);
                        self.input = String::new();
                    }
                    ChatEvent::MessageAdded(message) => {
                        self.history.push(message);
                    }
                    ChatEvent::LastMessageChanged(new_message) => {
                        if let Some(message) = self.history.last_mut() {
                            *message = new_message;
                        }
                    }
                    ChatEvent::ExchangeOver => {
                        self.is_sending = false;
                    }
                }

                Task::none()
            }
            Message::Chatting(Err(error)) => {
                self.error = Some(dbg!(error));
                self.is_sending = false;

                Task::none()
            }
        }
    }

    pub fn view(&self) -> Element<Message> {
        let title = text!("{name}", name = self.assistant.name())
            .font(Font::MONOSPACE)
            .size(20);

        let messages: Element<_> = if self.history.is_empty() {
            center(
                column![
                    text("Your assistant is ready."),
                    text("Say something! ↓").style(text::primary)
                ]
                .spacing(10)
                .align_items(Alignment::Center),
            )
            .into()
        } else {
            scrollable(
                column(self.history.iter().map(message_bubble))
                    .spacing(10)
                    .padding(Padding {
                        right: 20.0,
                        ..Padding::ZERO
                    }),
            )
            .align_y(scrollable::Alignment::End)
            .height(Length::Fill)
            .into()
        };

        let input = {
            let editor = text_input("Type your message here...", &self.input)
                .on_input(Message::InputChanged)
                .padding(10);

            if self.is_sending {
                editor
            } else {
                editor.on_submit(Message::InputSubmitted)
            }
        };

        column![title, messages, input]
            .spacing(10)
            .padding(10)
            .align_items(Alignment::Center)
            .into()
    }
}

fn message_bubble<'a>(message: &'a assistant::Message) -> Element<'a, Message> {
    container(
        container(text(message.content()))
            .width(Length::Fill)
            .style(move |theme: &Theme| {
                let palette = theme.extended_palette();

                let (background, border) = match message {
                    assistant::Message::Assistant(_) => (
                        palette.background.weak,
                        Border::rounded([0.0, 10.0, 10.0, 10.0]),
                    ),
                    assistant::Message::User(_) => (
                        palette.success.weak,
                        Border::rounded([10.0, 0.0, 10.0, 10.0]),
                    ),
                };

                container::Style {
                    background: Some(background.color.into()),
                    text_color: Some(background.text),
                    border,
                    ..container::Style::default()
                }
            })
            .padding(10),
    )
    .padding(match message {
        assistant::Message::Assistant(_) => Padding {
            right: 20.0,
            ..Padding::ZERO
        },
        assistant::Message::User(_) => Padding {
            left: 20.0,
            ..Padding::ZERO
        },
    })
    .into()
}
