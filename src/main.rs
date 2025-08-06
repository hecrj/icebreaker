#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
use icebreaker_core as core;

mod browser;
pub mod config;
mod icon;
mod screen;
mod ui;
mod widget;

use crate::core::assistant;
use crate::core::model;
use crate::core::{Chat, Error};
use crate::screen::conversation;
use crate::screen::search;
use crate::screen::settings;
use crate::screen::Screen;

use iced::system;
use iced::widget::{button, column, container, row, rule, vertical_rule, vertical_space, Text};
use iced::{Center, Element, Fill, Subscription, Task, Theme};

use std::mem;

pub fn main() -> iced::Result {
    tracing_subscriber::fmt::init();

    iced::application(Icebreaker::new, Icebreaker::update, Icebreaker::view)
        .title(Icebreaker::title)
        .subscription(Icebreaker::subscription)
        .theme(Icebreaker::theme)
        .font(icon::FONT)
        .run()
}

struct Icebreaker {
    screen: Screen,
    library: model::Library,
    last_conversation: Option<screen::Conversation>,
    system: Option<system::Information>,
    config: config::Config,
}

#[derive(Debug, Clone)]
enum Message {
    Loaded {
        last_chat: Result<Chat, Error>,
        system: Box<system::Information>,
        config: Result<config::Config, config::Error>,
    },
    Scanned(Result<model::Library, Error>),
    Escape,
    Search(search::Message),
    Conversation(conversation::Message),
    Settings(settings::Message),
    Chats,
    Discover,
}

impl Icebreaker {
    pub fn new() -> (Self, Task<Message>) {
        (
            Self {
                screen: Screen::Loading,
                library: model::Library::default(),
                last_conversation: None,
                system: None,
                config: config::Config::default(),
            },
            Task::batch([
                Task::future(async {
                    iced::futures::join!(Chat::fetch_last_opened(), config::Config::load())
                })
                .then(|(last_chat, config)| {
                    system::fetch_information()
                        .map(Box::new)
                        .map(move |system| Message::Loaded {
                            last_chat: last_chat.clone(),
                            config: config.clone(),
                            system,
                        })
                }),
                Task::perform(model::Library::scan(), Message::Scanned),
            ]),
        )
    }

    fn title(&self) -> String {
        match &self.screen {
            Screen::Loading => "Icebreaker".to_owned(),
            Screen::Search(search) => search.title(),
            Screen::Conversation(conversation) => conversation.title(),
            Screen::Settings(settings) => settings.title(),
        }
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Settings(settings_message) => {
                if let Screen::Settings(settings) = &mut self.screen {
                    let action = settings.update(settings_message);

                    match action {
                        settings::Action::None => Task::none(),
                        settings::Action::Run(task) => task.map(Message::Settings),
                    }
                } else {
                    Task::none()
                }
            }
            Message::Loaded {
                last_chat,
                system,
                config,
            } => {
                let backend = assistant::Backend::detect(&system.graphics_adapter);
                self.system = Some(*system);

                match config {
                    Ok(config) => {
                        self.config = config;
                    }
                    Err(error) => {
                        log::error!("Failed to load configuration: {error}");
                    }
                }

                match last_chat {
                    Ok(last_chat) => {
                        let (conversation, task) = screen::Conversation::open(last_chat, backend);

                        self.screen = Screen::Conversation(conversation);

                        task.map(Message::Conversation)
                    }
                    Err(error) => {
                        log::warn!("{error}");

                        self.search()
                    }
                }
            }
            Message::Scanned(Ok(library)) => {
                self.library = library;

                Task::none()
            }
            Message::Search(message) => {
                if let Screen::Search(search) = &mut self.screen {
                    let action = search.update(message);

                    match action {
                        search::Action::None => Task::none(),
                        search::Action::Run(task) => task.map(Message::Search),
                        search::Action::Boot(file) => {
                            let backend = self
                                .system
                                .as_ref()
                                .map(|system| assistant::Backend::detect(&system.graphics_adapter))
                                .unwrap_or(assistant::Backend::Cpu);

                            let (conversation, task) = screen::Conversation::new(file, backend);

                            self.screen = Screen::Conversation(conversation);

                            task.map(Message::Conversation)
                        }
                    }
                } else {
                    Task::none()
                }
            }
            Message::Conversation(message) => {
                let conversation = if let Screen::Conversation(conversation) = &mut self.screen {
                    Some(conversation)
                } else {
                    self.last_conversation.as_mut()
                };

                let Some(conversation) = conversation else {
                    return Task::none();
                };

                let action = conversation.update(message);

                match action {
                    conversation::Action::None => Task::none(),
                    conversation::Action::Run(task) => task.map(Message::Conversation),
                }
            }
            Message::Escape => {
                if matches!(self.screen, Screen::Search(_)) {
                    Task::none()
                } else {
                    self.search()
                }
            }
            Message::Chats => {
                if let Some(conversation) = self.last_conversation.take() {
                    self.screen = Screen::Conversation(conversation);
                }

                Task::none()
            }
            Message::Discover => {
                if let Screen::Conversation(conversation) =
                    mem::replace(&mut self.screen, Screen::Loading)
                {
                    self.last_conversation = Some(conversation);
                }

                self.search()
            }
            Message::Scanned(Err(error)) => {
                log::error!("{error}");

                Task::none()
            }
        }
    }

    fn view(&self) -> Element<'_, Message> {
        let sidebar = {
            let content = match &self.screen {
                Screen::Conversation(conversation) => {
                    conversation.sidebar().map(Message::Conversation)
                }
                Screen::Search(search) => search.sidebar(&self.library).map(Message::Search),
                _ => vertical_space().into(),
            };

            let tab = |icon: Text<'static>, toggled, message| {
                button(icon.width(Fill).align_x(Center))
                    .padding([10, 0])
                    .on_press_maybe(message)
                    .width(Fill)
                    .style(move |theme: &Theme, status| {
                        let palette = theme.extended_palette();

                        let base = button::text(theme, status);

                        if toggled {
                            button::Style {
                                background: Some(palette.background.weakest.color.into()),
                                text_color: palette.background.weakest.text,
                                ..base
                            }
                        } else {
                            base
                        }
                    })
            };

            let tabs = container(row![
                tab(
                    icon::chat(),
                    matches!(self.screen, Screen::Conversation(_)),
                    self.last_conversation.is_some().then_some(Message::Chats),
                ),
                tab(
                    icon::cubes(),
                    matches!(self.screen, Screen::Search(_)),
                    Some(Message::Discover),
                ),
            ])
            .style(|theme| {
                container::Style::default()
                    .background(theme.extended_palette().background.base.color)
            });

            row![
                container(column![container(content).padding(10), tabs])
                    .width(250)
                    .style(|theme| {
                        container::Style::default()
                            .background(theme.extended_palette().background.weakest.color)
                    }),
                vertical_rule(1).style(rule::weak),
            ]
        };

        let screen = match &self.screen {
            Screen::Loading => screen::loading(),
            Screen::Search(search) => search.view(&self.library).map(Message::Search),
            Screen::Conversation(conversation) => {
                conversation.view(&self.theme()).map(Message::Conversation)
            }
            Screen::Settings(settings) => settings.view().map(Message::Settings),
        };

        row![sidebar, container(screen).padding(10)].into()
    }

    fn subscription(&self) -> Subscription<Message> {
        use iced::keyboard;

        let screen = match &self.screen {
            Screen::Loading => Subscription::none(),
            Screen::Search(_) => Subscription::none(),
            Screen::Conversation(conversation) => {
                conversation.subscription().map(Message::Conversation)
            }
            Screen::Settings(_) => Subscription::none(),
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

        Task::batch([
            Task::perform(model::Library::scan(), Message::Scanned),
            task.map(Message::Search),
        ])
    }
}
