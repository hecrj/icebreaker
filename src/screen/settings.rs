use std::path::PathBuf;

use iced::{Element, Task};

pub struct Settings {}

#[derive(Debug, Clone)]
pub enum Message {
    None,
}

pub enum Action {
    None,
    Run(Task<Message>),
}

impl Settings {
    pub fn new() -> Self {
        Settings {}
    }

    pub fn title(&self) -> String {
        "Settings - Icebreaker".to_owned()
    }

    pub fn update(&mut self, message: Message) -> Action {
        match message {
            Message::None => Action::None,
        }
    }

    pub fn view<'a>(&'a self) -> Element<'a, Message> {
        iced::widget::text("Settings screen is under construction.").into()
    }
}
