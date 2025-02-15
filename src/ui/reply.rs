use crate::core::assistant;
use crate::ui::markdown;
use crate::ui::{Markdown, Reasoning};

use iced::widget::column;
use iced::{Element, Theme};

#[derive(Debug, Default)]
pub struct Reply {
    reasoning: Option<Reasoning>,
    content: String,
    markdown: Markdown,
}

impl Reply {
    pub fn from_data(reply: assistant::Reply) -> Self {
        Self {
            reasoning: reply.reasoning.map(Reasoning::from_data),
            markdown: Markdown::parse(&reply.content),
            content: reply.content,
        }
    }

    pub fn to_data(&self) -> assistant::Reply {
        assistant::Reply {
            reasoning: self.reasoning.as_ref().map(Reasoning::to_data),
            content: self.content.as_str().to_owned(),
            last_token: None,
        }
    }

    pub fn to_text(&self) -> String {
        match &self.reasoning {
            Some(reasoning) if reasoning.show => {
                format!(
                    "{reasoning}\n\n{content}",
                    reasoning = reasoning
                        .thoughts
                        .iter()
                        .map(|thought| format!("> {thought}"))
                        .collect::<Vec<_>>()
                        .join("\n>\n"),
                    content = self.content.as_str()
                )
            }
            _ => self.content.as_str().to_owned(),
        }
    }

    pub fn update(&mut self, new_reply: assistant::Reply) {
        self.reasoning = new_reply.reasoning.map(Reasoning::from_data);
        self.content = new_reply.content;

        if let Some(reasoning) = &mut self.reasoning {
            reasoning.show = new_reply.last_token.is_none();
        }

        if let Some(token) = new_reply.last_token {
            self.markdown.push_str(&token);
        }
    }

    pub fn toggle_reasoning(&mut self, show: bool) {
        if let Some(reasoning) = &mut self.reasoning {
            reasoning.show = show;
        }
    }

    pub fn view<Message>(
        &self,
        theme: &Theme,
        on_reasoning_toggle: impl Fn(bool) -> Message,
        on_markdown_interaction: impl Fn(markdown::Interaction) -> Message + 'static,
    ) -> Element<Message>
    where
        Message: Clone + 'static,
    {
        let message = self.markdown.view(theme).map(on_markdown_interaction);

        if let Some(reasoning) = &self.reasoning {
            column![reasoning.quote(on_reasoning_toggle), message]
                .spacing(20)
                .into()
        } else {
            message.into()
        }
    }
}
