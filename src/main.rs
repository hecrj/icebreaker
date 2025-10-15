#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
use icebreaker_core as core;

mod browser;
mod icon;
mod screen;
mod theme;
mod ui;
mod widget;

use crate::core::assistant;
use crate::core::model;
use crate::core::{Chat, Error, Settings};
use crate::screen::conversation;
use crate::screen::search;
use crate::screen::settings;
use crate::screen::Screen;

use iced::system;
use iced::widget::{button, column, container, row, rule, space, Text};
use iced::{Element, Fill, Subscription, Task, Theme};

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
    last_conversation: Option<screen::Conversation>,
    system: Option<system::Information>,
    library: model::Library,
    theme: Theme,
}

#[derive(Debug, Clone)]
enum Message {
    Loaded {
        last_chat: Result<Chat, Error>,
        system: Box<system::Information>,
    },
    Scanned(Result<model::Library, Error>),
    Escape,
    Search(search::Message),
    Conversation(conversation::Message),
    Settings(settings::Message),
    OpenChats,
    OpenSearch,
    OpenSettings,
    SettingsSaved(Result<(), Error>),
}

impl Icebreaker {
    pub fn new() -> (Self, Task<Message>) {
        let settings = Settings::fetch().unwrap_or_default();
        let scan = model::Library::scan(settings.library);

        (
            Self {
                screen: Screen::Loading,
                library: model::Library::default(),
                last_conversation: None,
                system: None,
                theme: theme::from_data(&settings.theme),
            },
            Task::batch([
                Task::future(Chat::fetch_last_opened()).then(|last_chat| {
                    system::information()
                        .map(Box::new)
                        .map(move |system| Message::Loaded {
                            last_chat: last_chat.clone(),
                            system,
                        })
                }),
                Task::perform(scan, Message::Scanned),
            ]),
        )
    }

    fn title(&self) -> String {
        let title = match &self.screen {
            Screen::Loading => return "Icebreaker".to_owned(),
            Screen::Search(search) => search.title(),
            Screen::Conversation(conversation) => conversation.title(),
            Screen::Settings(settings) => settings.title(),
        };

        format!("{title} - Icebreaker")
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Loaded { last_chat, system } => {
                let backend = assistant::Backend::detect(&system.graphics_adapter);
                self.system = Some(*system);

                match last_chat {
                    Ok(last_chat) => {
                        let (conversation, task) =
                            screen::Conversation::open(&self.library, last_chat, backend);

                        self.screen = Screen::Conversation(conversation);

                        task.map(Message::Conversation)
                    }
                    Err(error) => {
                        log::warn!("{error}");

                        self.open_search()
                    }
                }
            }
            Message::Scanned(Ok(library)) => {
                let old_library = std::mem::replace(&mut self.library, library);

                if old_library.directory() != self.library.directory() {
                    self.save_settings()
                } else {
                    Task::none()
                }
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

                            let (conversation, task) =
                                screen::Conversation::new(&self.library, file, backend);

                            self.screen = Screen::Conversation(conversation);
                            self.last_conversation = None;

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

                let action = conversation.update(&self.library, message);

                match action {
                    conversation::Action::None => Task::none(),
                    conversation::Action::Run(task) => task.map(Message::Conversation),
                }
            }
            Message::Settings(message) => {
                let Screen::Settings(settings) = &mut self.screen else {
                    return Task::none();
                };

                match settings.update(message) {
                    settings::Action::None => Task::none(),
                    settings::Action::ChangeTheme(theme) => {
                        self.theme = theme;

                        self.save_settings()
                    }
                    settings::Action::ChangeLibraryFolder(library) => {
                        Task::perform(model::Library::scan(library), Message::Scanned)
                    }
                    settings::Action::Run(task) => task.map(Message::Settings),
                }
            }
            Message::Escape => {
                if matches!(self.screen, Screen::Search(_)) {
                    Task::none()
                } else {
                    self.open_search()
                }
            }
            Message::OpenChats => {
                if let Some(conversation) = self.last_conversation.take() {
                    self.screen = Screen::Conversation(conversation);
                }

                Task::none()
            }
            Message::OpenSearch => {
                if let Screen::Conversation(conversation) =
                    mem::replace(&mut self.screen, Screen::Loading)
                {
                    self.last_conversation = Some(conversation);
                }

                self.open_search()
            }
            Message::OpenSettings => {
                if let Screen::Conversation(conversation) =
                    mem::replace(&mut self.screen, Screen::Loading)
                {
                    self.last_conversation = Some(conversation);
                }

                self.open_settings()
            }
            Message::SettingsSaved(Ok(())) => Task::none(),
            Message::Scanned(Err(error)) | Message::SettingsSaved(Err(error)) => {
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
                Screen::Settings(settings) => settings.sidebar().map(Message::Settings),
                Screen::Loading => space::vertical().into(),
            };

            let tab = |icon: Text<'static>, toggled, message| {
                button(icon.width(Fill).height(Fill).center())
                    .padding(0)
                    .height(40)
                    .on_press_maybe(message)
                    .width(Fill)
                    .style(move |theme: &Theme, status| {
                        let palette = theme.extended_palette();

                        let base = button::text(theme, status);

                        if toggled {
                            button::Style {
                                text_color: palette.background.neutral.text,
                                background: Some(palette.background.neutral.color.into()),
                                border: base.border.rounded(10),
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
                    self.last_conversation
                        .is_some()
                        .then_some(Message::OpenChats),
                ),
                tab(
                    icon::cubes(),
                    matches!(self.screen, Screen::Search(_)),
                    Some(Message::OpenSearch),
                ),
                tab(
                    icon::cog(),
                    matches!(self.screen, Screen::Settings(_)),
                    Some(Message::OpenSettings)
                ),
            ])
            .padding(10)
            .style(|theme| {
                container::Style::default()
                    .background(theme.extended_palette().background.weaker.color)
            });

            row![
                container(column![container(content).padding(10).height(Fill), tabs])
                    .width(250)
                    .style(|theme| {
                        container::Style::default()
                            .background(theme.extended_palette().background.weakest.color)
                    }),
                rule::vertical(1).style(rule::weak),
            ]
        };

        let screen = match &self.screen {
            Screen::Loading => screen::loading(),
            Screen::Search(search) => search.view(&self.library).map(Message::Search),
            Screen::Conversation(conversation) => {
                conversation.view(&self.theme).map(Message::Conversation)
            }
            Screen::Settings(settings) => settings
                .view(&self.library, &self.theme)
                .map(Message::Settings),
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
        self.theme.clone()
    }

    fn open_search(&mut self) -> Task<Message> {
        let (search, task) = screen::Search::new();

        self.screen = Screen::Search(search);

        Task::batch([
            Task::perform(
                model::Library::scan(self.library.directory().clone()),
                Message::Scanned,
            ),
            task.map(Message::Search),
        ])
    }

    fn open_settings(&mut self) -> Task<Message> {
        let (settings, task) = screen::Settings::new();

        self.screen = Screen::Settings(settings);

        task.map(Message::Settings)
    }

    fn save_settings(&self) -> Task<Message> {
        let settings = Settings {
            library: self.library.directory().clone(),
            theme: theme::to_data(&self.theme),
        };

        Task::perform(settings.save(), Message::SettingsSaved)
    }
}
