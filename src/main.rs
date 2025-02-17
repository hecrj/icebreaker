#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
use icebreaker_core as core;

mod browser;
mod icon;
mod screen;
mod ui;
mod widget;

use crate::core::assistant;
use crate::core::{Chat, Error};
use crate::screen::boot;
use crate::screen::conversation;
use crate::screen::search;
use crate::screen::Screen;

use iced::system;
use iced::{Element, Subscription, Task, Theme};

pub fn main() -> iced::Result {
    tracing_subscriber::fmt::init();

    iced::application(Icebreaker::title, Icebreaker::update, Icebreaker::view)
        .font(icon::FONT)
        .subscription(Icebreaker::subscription)
        .theme(Icebreaker::theme)
        .run_with(Icebreaker::new)
}

struct Icebreaker {
    screen: Screen,
    system: Option<system::Information>,
}

#[derive(Debug, Clone)]
enum Message {
    Loaded {
        last_chat: Result<Chat, Error>,
        system: Box<system::Information>,
    },
    Escape,
    Search(search::Message),
    Boot(boot::Message),
    Conversation(conversation::Message),
}

impl Icebreaker {
    pub fn new() -> (Self, Task<Message>) {
        (
            Self {
                screen: Screen::Loading,
                system: None,
            },
            Task::future(Chat::fetch_last_opened()).then(|last_chat| {
                system::fetch_information()
                    .map(Box::new)
                    .map(move |system| Message::Loaded {
                        last_chat: last_chat.clone(),
                        system,
                    })
            }),
        )
    }

    fn title(&self) -> String {
        match &self.screen {
            Screen::Loading => "Icebreaker".to_owned(),
            Screen::Search(search) => search.title(),
            Screen::Boot(boot) => boot.title(),
            Screen::Conversation(conversation) => conversation.title(),
        }
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Loaded { last_chat, system } => {
                let backend = assistant::Backend::detect(&system.graphics_adapter);
                self.system = Some(*system);

                match last_chat {
                    Ok(last_chat) => {
                        let (conversation, task) = screen::Conversation::open(last_chat, backend);

                        self.screen = Screen::Conversation(conversation);

                        task.map(Message::Conversation)
                    }
                    Err(error) => {
                        log::error!("{error}");

                        self.search()
                    }
                }
            }
            Message::Search(message) => {
                if let Screen::Search(search) = &mut self.screen {
                    let action = search.update(message);

                    match action {
                        search::Action::None => Task::none(),
                        search::Action::Run(task) => task.map(Message::Search),
                        search::Action::Boot(model) => {
                            let (boot, task) = screen::Boot::new(model, self.system.as_ref());

                            self.screen = Screen::Boot(boot);

                            task.map(Message::Boot)
                        }
                    }
                } else {
                    Task::none()
                }
            }
            Message::Boot(message) => {
                if let Screen::Boot(search) = &mut self.screen {
                    let action = search.update(message);

                    match action {
                        boot::Action::None => Task::none(),
                        boot::Action::Boot { file, backend } => {
                            let (conversation, task) = screen::Conversation::new(file, backend);

                            self.screen = Screen::Conversation(conversation);

                            task.map(Message::Conversation)
                        }
                        boot::Action::Abort => self.search(),
                    }
                } else {
                    Task::none()
                }
            }
            Message::Conversation(message) => {
                if let Screen::Conversation(conversation) = &mut self.screen {
                    let action = conversation.update(message);

                    match action {
                        conversation::Action::None => Task::none(),
                        conversation::Action::Run(task) => task.map(Message::Conversation),
                        conversation::Action::Back => self.search(),
                    }
                } else {
                    Task::none()
                }
            }
            Message::Escape => {
                if matches!(self.screen, Screen::Search(_)) {
                    Task::none()
                } else {
                    self.search()
                }
            }
        }
    }

    fn view(&self) -> Element<Message> {
        match &self.screen {
            Screen::Loading => screen::loading(),
            Screen::Search(search) => search.view().map(Message::Search),
            Screen::Boot(boot) => boot.view(self.theme()).map(Message::Boot),
            Screen::Conversation(conversation) => {
                conversation.view(&self.theme()).map(Message::Conversation)
            }
        }
    }

    fn subscription(&self) -> Subscription<Message> {
        use iced::keyboard;

        let screen = match &self.screen {
            Screen::Loading => Subscription::none(),
            Screen::Search(search) => search.subscription().map(Message::Search),
            Screen::Boot(_) => Subscription::none(),
            Screen::Conversation(conversation) => {
                conversation.subscription().map(Message::Conversation)
            }
        };

        let hotkeys = keyboard::on_key_press(|key, _modifiers| match key {
            keyboard::Key::Named(keyboard::key::Named::Escape) => Some(Message::Escape),
            _ => None,
        });

        Subscription::batch([screen, hotkeys])
    }

    fn theme(&self) -> Theme {
        Theme::TokyoNight
    }

    fn search(&mut self) -> Task<Message> {
        let (search, task) = screen::Search::new();

        self.screen = Screen::Search(search);

        task.map(Message::Search)
    }
}
