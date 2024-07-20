use crate::data::assistant::{self, Assistant, Backend, BootEvent, File};
use crate::data::chat::{self, Chat, Entry, Id};
use crate::data::Error;
use crate::icon;
use crate::widget::tip;

use iced::border;
use iced::clipboard;
use iced::padding;
use iced::task::{self, Task};
use iced::time::{self, Duration, Instant};
use iced::widget::{
    self, button, center, column, container, horizontal_space, hover, progress_bar, row,
    scrollable, stack, text, text_input, tooltip, value,
};
use iced::{Center, Element, Fill, Font, Left, Right, Subscription, Theme};

pub struct Conversation {
    backend: Backend,
    chats: Vec<Entry>,
    state: State,
    id: Option<Id>,
    title: Option<String>,
    history: Vec<assistant::Message>,
    input: String,
    error: Option<Error>,
    sidebar_open: bool,
}

enum State {
    Booting {
        file: File,
        logs: Vec<String>,
        stage: String,
        progress: u64,
        tick: usize,
        _task: task::Handle,
    },
    Running {
        assistant: Assistant,
        sending: Option<task::Handle>,
    },
}

#[derive(Debug, Clone)]
pub enum Message {
    ChatsListed(Result<Vec<Entry>, Error>),
    Booting(Result<BootEvent, Error>),
    Tick(Instant),
    InputChanged(String),
    InputSubmitted,
    Chatting(Result<chat::Event, Error>),
    Copy(assistant::Message),
    Created(Result<Chat, Error>),
    Saved(Result<Chat, Error>),
    Open(chat::Id),
    ChatFetched(Result<Chat, Error>),
    Delete,
    New,
    Search,
    ToggleSidebar,
}

pub enum Action {
    None,
    Run(Task<Message>),
    Back,
}

impl Conversation {
    pub fn new(file: File, backend: Backend) -> (Self, Task<Message>) {
        let (boot, handle) =
            Task::run(Assistant::boot(file.clone(), backend), Message::Booting).abortable();

        (
            Self {
                backend,
                state: State::Booting {
                    file,
                    logs: Vec::new(),
                    stage: "Booting...".to_owned(),
                    progress: 0,
                    tick: 0,
                    _task: handle.abort_on_drop(),
                },
                id: None,
                title: None,
                history: Vec::new(),
                input: String::new(),
                error: None,
                chats: Vec::new(),
                sidebar_open: true,
            },
            Task::batch([
                boot,
                Task::perform(Chat::list(), Message::ChatsListed),
                widget::focus_next(),
            ]),
        )
    }

    pub fn open(chat: Chat, backend: Backend) -> (Self, Task<Message>) {
        let (conversation, task) = Self::new(chat.file, backend);

        (
            Self {
                id: Some(chat.id),
                title: chat.title,
                history: chat.history,
                ..conversation
            },
            task,
        )
    }

    pub fn title(&self) -> String {
        format!("{name} - Icebreaker", name = self.model_name())
    }

    pub fn update(&mut self, message: Message) -> Action {
        match message {
            Message::ChatsListed(Ok(chats)) => {
                self.chats = chats;

                Action::None
            }
            Message::ChatsListed(Err(error)) => {
                self.error = Some(dbg!(error));

                Action::None
            }
            Message::Booting(Ok(event)) => match event {
                BootEvent::Progressed {
                    stage: new_stage,
                    percent,
                } => {
                    if let State::Booting {
                        stage, progress, ..
                    } = &mut self.state
                    {
                        new_stage.clone_into(stage);
                        *progress = percent;
                    }

                    Action::None
                }
                BootEvent::Logged(log) => {
                    if let State::Booting { logs, .. } = &mut self.state {
                        logs.push(log);
                    }

                    Action::None
                }
                BootEvent::Finished(assistant) => {
                    self.state = State::Running {
                        assistant,
                        sending: None,
                    };

                    Action::None
                }
            },
            Message::Booting(Err(error)) => {
                self.error = Some(error);

                Action::None
            }
            Message::Tick(_now) => {
                if let State::Booting { tick, .. } = &mut self.state {
                    *tick += 1;
                }

                Action::None
            }
            Message::InputChanged(input) => {
                self.input = input;
                self.error = None;

                Action::None
            }
            Message::InputSubmitted => {
                if let State::Running { assistant, sending } = &mut self.state {
                    let (send, handle) = Task::run(
                        chat::send(assistant, &self.history, &self.input),
                        Message::Chatting,
                    )
                    .abortable();

                    *sending = Some(handle.abort_on_drop());

                    Action::Run(send)
                } else {
                    Action::None
                }
            }
            Message::Chatting(Ok(event)) if !self.can_send() => match event {
                chat::Event::TitleChanged(title) => {
                    self.title = Some(title);

                    Action::None
                }
                chat::Event::MessageSent(message) => {
                    self.history.push(message);
                    self.input = String::new();

                    Action::None
                }
                chat::Event::MessageAdded(message) => {
                    self.history.push(message);

                    Action::None
                }
                chat::Event::LastMessageChanged(new_message) => {
                    if let Some(message) = self.history.last_mut() {
                        *message = new_message;
                    }

                    Action::None
                }
                chat::Event::ExchangeOver => {
                    if let State::Running {
                        sending, assistant, ..
                    } = &mut self.state
                    {
                        *sending = None;

                        if let Some(id) = &self.id {
                            Action::Run(Task::perform(
                                Chat::save(
                                    id.clone(),
                                    assistant.file().clone(),
                                    self.title.clone(),
                                    self.history.clone(),
                                ),
                                Message::Saved,
                            ))
                        } else {
                            Action::Run(Task::perform(
                                Chat::create(
                                    assistant.file().clone(),
                                    self.title.clone(),
                                    self.history.clone(),
                                ),
                                Message::Created,
                            ))
                        }
                    } else {
                        Action::None
                    }
                }
            },
            Message::Chatting(Ok(_outdated_event)) => Action::None,
            Message::Chatting(Err(error)) => {
                self.error = Some(dbg!(error));

                if let State::Running { sending, .. } = &mut self.state {
                    *sending = None;
                }

                Action::None
            }
            Message::Copy(message) => Action::Run(clipboard::write(message.content().to_owned())),
            Message::Created(Ok(chat)) | Message::Saved(Ok(chat)) => {
                self.id = Some(chat.id);

                Action::Run(Task::perform(Chat::list(), Message::ChatsListed))
            }
            Message::Created(Err(error)) | Message::Saved(Err(error)) => {
                self.error = Some(dbg!(error));

                Action::None
            }
            Message::Open(chat) => {
                Action::Run(Task::perform(Chat::fetch(chat), Message::ChatFetched))
            }
            Message::ChatFetched(Ok(chat)) => match &mut self.state {
                State::Booting { file, .. } if file == &chat.file => {
                    self.id = Some(chat.id);
                    self.title = chat.title;
                    self.history = chat.history;
                    self.input = String::new();

                    Action::Run(widget::focus_next())
                }
                State::Running { assistant, sending } if assistant.file() == &chat.file => {
                    self.id = Some(chat.id);
                    self.title = chat.title;
                    self.history = chat.history;
                    self.input = String::new();
                    self.error = None;

                    *sending = None;

                    Action::Run(widget::focus_next())
                }
                _ => {
                    let (conversation, task) = Self::open(chat, self.backend);

                    *self = conversation;

                    Action::Run(task)
                }
            },
            Message::ChatFetched(Err(error)) => {
                self.error = Some(dbg!(error));

                Action::None
            }
            Message::New => {
                self.id = None;
                self.title = None;
                self.history = Vec::new();
                self.input = String::new();
                self.error = None;

                if let State::Running { sending, .. } = &mut self.state {
                    *sending = None;
                }

                Action::Run(widget::focus_next())
            }
            Message::Delete => {
                if let Some(id) = self.id.clone() {
                    Action::Run(Task::future(Chat::delete(id)).and_then(|_| {
                        Task::batch([
                            Task::perform(Chat::fetch_last_opened(), Message::ChatFetched),
                            Task::perform(Chat::list(), Message::ChatsListed),
                        ])
                    }))
                } else {
                    Action::None
                }
            }
            Message::Search => Action::Back,
            Message::ToggleSidebar => {
                self.sidebar_open = !self.sidebar_open;

                Action::None
            }
        }
    }

    pub fn view(&self) -> Element<Message> {
        let header: Element<_> = {
            let title: Element<_> = match &self.title {
                Some(title) => column![
                    text(title).font(Font::MONOSPACE).size(20),
                    text(self.model_name())
                        .font(Font::MONOSPACE)
                        .size(14)
                        .style(text::secondary)
                ]
                .spacing(5)
                .align_x(Center)
                .width(Fill)
                .into(),
                None => text(self.model_name())
                    .font(Font::MONOSPACE)
                    .size(20)
                    .width(Fill)
                    .align_x(Center)
                    .into(),
            };

            let toggle_sidebar = tip(
                button(if self.sidebar_open {
                    icon::collapse()
                } else {
                    icon::expand()
                })
                .padding(0)
                .on_press(Message::ToggleSidebar)
                .style(button::text),
                if self.sidebar_open {
                    "Close sidebar"
                } else {
                    "Open sidebar"
                },
                tip::Position::Right,
            );

            let delete = tip(
                button(icon::trash().style(text::danger))
                    .padding(0)
                    .on_press(Message::Delete)
                    .style(button::text),
                "Delete Chat",
                tip::Position::Left,
            );

            let bar = stack![title, row![toggle_sidebar, horizontal_space(), delete]].into();

            match &self.state {
                State::Booting {
                    logs,
                    stage,
                    progress,
                    tick,
                    ..
                } => {
                    let progress = {
                        let stage = if self.error.is_none() {
                            text!(
                                "{stage} {spinner}",
                                stage = stage,
                                spinner = match tick % 4 {
                                    0 => "|",
                                    1 => "/",
                                    2 => "—",
                                    _ => "\\",
                                }
                            )
                        } else {
                            text(stage)
                        }
                        .font(Font::MONOSPACE)
                        .size(10);

                        let bar = progress_bar(0.0..=100.0, *progress as f32)
                            .width(200)
                            .height(30);

                        stack![
                            if self.error.is_none() {
                                bar
                            } else {
                                bar.style(progress_bar::danger)
                            },
                            center(stage.style(|theme: &Theme| text::Style {
                                color: Some(theme.palette().background)
                            }))
                            .clip(true)
                        ]
                    };

                    let logs = {
                        let error = self
                            .error
                            .as_ref()
                            .map(|error| value(error).font(Font::MONOSPACE).style(text::danger));

                        scrollable(
                            column(
                                logs.iter()
                                    .map(|log| text(log).size(12).font(Font::MONOSPACE).into()),
                            )
                            .push_maybe(error)
                            .spacing(5)
                            .padding(padding::right(20)),
                        )
                        .anchor_y(scrollable::Anchor::End)
                        .width(400)
                        .height(600)
                    };

                    let progress = tooltip(
                        progress,
                        container(logs).padding(10).style(container::dark),
                        tooltip::Position::Bottom,
                    );

                    stack![bar, container(progress).align_right(Fill).align_y(Center)].into()
                }
                State::Running { .. } => bar,
            }
        };

        let messages: Element<_> = if self.history.is_empty() {
            center(
                match &self.state {
                    State::Running { .. } => column![
                        text("Your assistant is ready."),
                        text("Break the ice! ↓").style(text::primary),
                    ],
                    State::Booting { .. } => column![
                        text("Your assistant is launching..."),
                        text("You can begin typing while you wait! ↓").style(text::success),
                    ],
                }
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

            if self.can_send() {
                editor.on_submit(Message::InputSubmitted)
            } else {
                editor
            }
        };

        let chat = column![header, messages, input].spacing(10).align_x(Center);

        if self.sidebar_open {
            let sidebar = {
                let chats = column(self.chats.iter().map(|chat| {
                    let card: Element<_> = match &chat.title {
                        Some(title) => {
                            let title = text(title).font(Font::MONOSPACE);
                            let subtitle =
                                text(chat.file.model.name()).font(Font::MONOSPACE).size(12);

                            column![title, subtitle].spacing(3).into()
                        }
                        None => text(chat.file.model.name()).font(Font::MONOSPACE).into(),
                    };

                    let is_active = Some(&chat.id) == self.id.as_ref();

                    if is_active {
                        container(card)
                            .style(|theme: &Theme| {
                                let pair = theme.extended_palette().secondary.weak;

                                container::Style {
                                    background: Some(pair.color.into()),
                                    text_color: Some(pair.text),
                                    border: border::rounded(2),
                                    ..container::Style::default()
                                }
                            })
                            .padding(5)
                            .width(Fill)
                            .into()
                    } else {
                        button(card)
                            .on_press_with(move || Message::Open(chat.id.clone()))
                            .padding(5)
                            .width(Fill)
                            .style(|theme: &Theme, status: button::Status| match status {
                                button::Status::Active => button::text(theme, status),
                                _ => button::secondary(theme, status),
                            })
                            .into()
                    }
                }))
                .clip(true)
                .spacing(10);

                let new = button(text("New Chat").width(Fill).align_x(Center))
                    .on_press(Message::New)
                    .style(button::success);

                let search = button(text("Search Models").width(Fill).align_x(Center))
                    .on_press(Message::Search)
                    .style(button::secondary);

                column![scrollable(chats).height(Fill), new, search]
                    .width(250)
                    .spacing(10)
            };

            row![sidebar, chat].spacing(10).padding(10).into()
        } else {
            chat.padding(10).into()
        }
    }

    pub fn subscription(&self) -> Subscription<Message> {
        match &self.state {
            State::Booting { .. } => time::every(Duration::from_millis(100)).map(Message::Tick),
            State::Running { .. } => Subscription::none(),
        }
    }

    pub fn model_name(&self) -> &str {
        match &self.state {
            State::Booting { file, .. } => file.model.name(),
            State::Running { assistant, .. } => assistant.name(),
        }
    }

    pub fn can_send(&self) -> bool {
        matches!(self.state, State::Running { sending: None, .. })
    }
}

fn message_bubble(message: &assistant::Message) -> Element<Message> {
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

    let copy = tip(
        button(icon::clipboard())
            .on_press_with(|| Message::Copy(message.clone()))
            .padding(0)
            .style(button::text),
        "Copy to clipboard",
        tip::Position::Bottom,
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
}
