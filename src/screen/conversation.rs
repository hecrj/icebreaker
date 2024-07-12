use crate::assistant::{self, Assistant, ChatError, ChatEvent};
use crate::icon;

use iced::clipboard;
use iced::padding;
use iced::widget::{
    self, button, center, column, container, hover, scrollable, text, text_input, tooltip,
};
use iced::{Center, Element, Fill, Font, Left, Right, Task, Theme};

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
    Copy(assistant::Message),
    Back,
}

pub enum Action {
    None,
    Run(Task<Message>),
    Back,
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

    pub fn title(&self) -> String {
        format!("{name} - Icebreaker", name = self.assistant.name())
    }

    pub fn update(&mut self, message: Message) -> Action {
        match message {
            Message::InputChanged(input) => {
                self.input = input;
                self.error = None;

                Action::None
            }
            Message::InputSubmitted => {
                self.is_sending = true;

                Action::Run(Task::run(
                    self.assistant.chat(&self.history, &self.input),
                    Message::Chatting,
                ))
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

                Action::None
            }
            Message::Chatting(Err(error)) => {
                self.error = Some(dbg!(error));
                self.is_sending = false;

                Action::None
            }
            Message::Copy(message) => Action::Run(clipboard::write(message.content().to_owned())),
            Message::Back => Action::Back,
        }
    }

    pub fn view(&self) -> Element<Message> {
        let header = {
            let title = text!("{name}", name = self.assistant.name())
                .font(Font::MONOSPACE)
                .size(20)
                .width(Fill)
                .align_x(Center);

            let back = tooltip(
                button(text("←").size(20))
                    .padding(0)
                    .on_press(Message::Back)
                    .style(button::text),
                container(text("Back to search").size(14))
                    .padding(5)
                    .style(container::rounded_box),
                tooltip::Position::Right,
            );

            hover(title, container(back).center_y(Fill))
        };

        let messages: Element<_> = if self.history.is_empty() {
            center(
                column![
                    text("Your assistant is ready."),
                    text("Break the ice! ↓").style(text::primary)
                ]
                .spacing(10)
                .align_x(Center),
            )
            .into()
        } else {
            scrollable(
                column(self.history.iter().map(message_bubble))
                    .spacing(10)
                    .padding(padding::right(20)),
            )
            .anchor_y(scrollable::Anchor::End)
            .height(Fill)
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

        column![header, messages, input]
            .spacing(10)
            .padding(10)
            .align_x(Center)
            .into()
    }
}

fn message_bubble<'a>(message: &'a assistant::Message) -> Element<'a, Message> {
    use iced::border;

    let bubble = container(
        container(text(message.content()))
            .width(Fill)
            .style(move |theme: &Theme| {
                let palette = theme.extended_palette();

                let (background, radius) = match message {
                    assistant::Message::Assistant(_) => {
                        (palette.background.weak, border::radius(10).top_left(0))
                    }
                    assistant::Message::User(_) => {
                        (palette.success.weak, border::radius(10.0).top_right(0))
                    }
                };

                container::Style {
                    background: Some(background.color.into()),
                    text_color: Some(background.text),
                    border: border::rounded(radius),
                    ..container::Style::default()
                }
            })
            .padding(10),
    )
    .padding(match message {
        assistant::Message::Assistant(_) => padding::right(20),
        assistant::Message::User(_) => padding::left(20),
    });

    let copy = tooltip(
        button(icon::clipboard())
            .on_press_with(|| Message::Copy(message.clone()))
            .padding(0)
            .style(button::text),
        container(text("Copy to clipboard").size(12))
            .padding(5)
            .style(|theme: &Theme| container::Style {
                background: Some(theme.extended_palette().secondary.weak.color.into()),
                ..container::rounded_box(theme)
            }),
        tooltip::Position::Bottom,
    );

    hover(
        bubble,
        container(copy)
            .width(Fill)
            .center_y(Fill)
            .align_x(match message {
                assistant::Message::Assistant(_) => Right,
                assistant::Message::User(_) => Left,
            }),
    )
    .into()
}
