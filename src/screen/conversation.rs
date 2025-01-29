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
    self, button, center, center_x, column, container, horizontal_space, hover, progress_bar,
    right, right_center, row, scrollable, stack, text, text_editor, tooltip, value, vertical_rule,
};
use iced::{Center, Element, Fill, Font, Left, Right, Shrink, Subscription, Theme};

pub struct Conversation {
    backend: Backend,
    chats: Vec<Entry>,
    state: State,
    id: Option<Id>,
    title: Option<String>,
    history: History,
    input: text_editor::Content,
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
    InputChanged(text_editor::Action),
    Submit,
    Chatting(Result<chat::Event, Error>),
    Copy(Item),
    ToggleReasoning(usize),
    Created(Result<Chat, Error>),
    Saved(Result<Chat, Error>),
    Open(chat::Id),
    ChatFetched(Result<Chat, Error>),
    LastChatFetched(Result<Chat, Error>),
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
                history: History::new(),
                input: text_editor::Content::new(),
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
                history: History::restore(chat.history),
                ..conversation
            },
            Task::batch([
                task,
                scrollable::snap_to(scrollable::Id::new("chat"), scrollable::RelativeOffset::END),
            ]),
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
            Message::InputChanged(action) => {
                self.input.perform(action);
                self.error = None;

                Action::None
            }
            Message::Submit => {
                if let State::Running { assistant, sending } = &mut self.state {
                    if let Some(message) = chat::Content::parse(&self.input.text()) {
                        let (send, handle) = Task::run(
                            chat::send(assistant, self.history.messages().collect(), message),
                            Message::Chatting,
                        )
                        .abortable();

                        *sending = Some(handle.abort_on_drop());

                        Action::Run(Task::batch([
                            send,
                            scrollable::snap_to(
                                scrollable::Id::new("chat"),
                                scrollable::RelativeOffset { x: 0.0, y: 1.0 },
                            ),
                        ]))
                    } else {
                        Action::None
                    }
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
                    self.input = text_editor::Content::new();

                    Action::None
                }
                chat::Event::MessageAdded(message) => {
                    self.history.push(message);

                    Action::None
                }
                chat::Event::LastMessageChanged(new_message) => {
                    self.history.replace_last(new_message);

                    Action::None
                }
                chat::Event::ExchangeOver => {
                    if let State::Running {
                        sending, assistant, ..
                    } = &mut self.state
                    {
                        *sending = None;

                        let messages = self.history.messages().collect();

                        if let Some(id) = &self.id {
                            Action::Run(Task::perform(
                                Chat::save(
                                    id.clone(),
                                    assistant.file().clone(),
                                    self.title.clone(),
                                    messages,
                                ),
                                Message::Saved,
                            ))
                        } else {
                            Action::Run(Task::perform(
                                Chat::create(
                                    assistant.file().clone(),
                                    self.title.clone(),
                                    messages,
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
            Message::Copy(message) => Action::Run(clipboard::write(message.into_text())),
            Message::ToggleReasoning(index) => {
                if let Some(Item::Assistant { show_reasoning, .. }) = self.history.get_mut(index) {
                    *show_reasoning = !*show_reasoning;
                }

                Action::None
            }
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
            Message::ChatFetched(Ok(chat)) | Message::LastChatFetched(Ok(chat)) => {
                match &mut self.state {
                    State::Booting { file, .. } if file == &chat.file => {
                        self.id = Some(chat.id);
                        self.title = chat.title;
                        self.history = History::restore(chat.history);
                        self.input = text_editor::Content::new();

                        Action::Run(Task::batch([
                            widget::focus_next(),
                            scrollable::snap_to(
                                scrollable::Id::new("chat"),
                                scrollable::RelativeOffset::END,
                            ),
                        ]))
                    }
                    State::Running { assistant, sending } if assistant.file() == &chat.file => {
                        self.id = Some(chat.id);
                        self.title = chat.title;
                        self.history = History::restore(chat.history);
                        self.input = text_editor::Content::new();
                        self.error = None;

                        *sending = None;

                        Action::Run(Task::batch([
                            widget::focus_next(),
                            scrollable::snap_to(
                                scrollable::Id::new("chat"),
                                scrollable::RelativeOffset::END,
                            ),
                        ]))
                    }
                    _ => {
                        let (conversation, task) = Self::open(chat, self.backend);

                        *self = conversation;

                        Action::Run(task)
                    }
                }
            }
            Message::ChatFetched(Err(error)) => {
                self.error = Some(dbg!(error));

                Action::None
            }
            Message::New | Message::LastChatFetched(Err(_)) => {
                self.id = None;
                self.title = None;
                self.history = History::new();
                self.input = text_editor::Content::new();
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
                            Task::perform(Chat::fetch_last_opened(), Message::LastChatFetched),
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
                .into(),
                None => text(self.model_name())
                    .font(Font::MONOSPACE)
                    .size(20)
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

            let delete: Element<_> = if self.id.is_some() {
                tip(
                    button(icon::trash().style(text::danger))
                        .padding(0)
                        .on_press(Message::Delete)
                        .style(button::text),
                    "Delete Chat",
                    tip::Position::Left,
                )
            } else {
                horizontal_space().into()
            };

            let bar = row![
                toggle_sidebar,
                horizontal_space(),
                title,
                horizontal_space(),
                delete
            ]
            .spacing(10)
            .into();

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
                            .length(200)
                            .girth(30);

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

                    stack![bar, right_center(container(progress))].into()
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
            scrollable(center_x(
                column(
                    self.history
                        .items()
                        .enumerate()
                        .map(|(i, item)| item.view(i)),
                )
                .spacing(40)
                .padding(20)
                .max_width(600),
            ))
            .id(scrollable::Id::new("chat"))
            .spacing(10)
            .height(Fill)
            .into()
        };

        let input = text_editor(&self.input)
            .placeholder("Type your message here...")
            .on_action(Message::InputChanged)
            .padding(10)
            .key_binding(|key_press| {
                let modifiers = key_press.modifiers;

                match text_editor::Binding::from_key_press(key_press) {
                    Some(text_editor::Binding::Enter) if !modifiers.shift() => {
                        Some(text_editor::Binding::Custom(Message::Submit))
                    }
                    binding => binding,
                }
            });

        let chat = column![header, messages, input].spacing(10).align_x(Center);

        if self.sidebar_open {
            let sidebar = {
                let chats = column(self.chats.iter().map(|chat| {
                    let card: Element<_> = match &chat.title {
                        Some(title) => {
                            let title = text(title).font(Font::MONOSPACE);
                            let subtitle =
                                text(chat.file.model.name()).font(Font::MONOSPACE).size(10);

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

                column![scrollable(chats).height(Fill).spacing(10), new, search]
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

impl From<assistant::Message> for Item {
    fn from(message: assistant::Message) -> Self {
        match message {
            assistant::Message::Assistant { reasoning, content } => Item::Assistant {
                reasoning,
                content,
                show_reasoning: true,
            },
            assistant::Message::User(content) => Item::User(content),
        }
    }
}

impl From<Item> for assistant::Message {
    fn from(item: Item) -> Self {
        match item {
            Item::User(content) => assistant::Message::User(content),
            Item::Assistant {
                reasoning, content, ..
            } => assistant::Message::Assistant { reasoning, content },
        }
    }
}

pub struct History {
    items: Vec<Item>,
}

impl History {
    pub fn new() -> Self {
        Self { items: Vec::new() }
    }

    pub fn restore(messages: impl IntoIterator<Item = assistant::Message>) -> Self {
        Self {
            items: messages.into_iter().map(Item::from).collect(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn items(&self) -> impl Iterator<Item = &Item> {
        self.items.iter()
    }

    pub fn get_mut(&mut self, index: usize) -> Option<&mut Item> {
        self.items.get_mut(index)
    }

    pub fn push(&mut self, item: impl Into<Item>) {
        self.items.push(item.into());
    }

    pub fn replace_last(&mut self, item: impl Into<Item>) {
        if let Some(last) = self.items.last_mut() {
            *last = item.into();
        }
    }

    pub fn messages<'a>(&'a self) -> impl Iterator<Item = assistant::Message> + 'a {
        self.items.iter().cloned().map(assistant::Message::from)
    }
}

#[derive(Debug, Clone)]
pub enum Item {
    User(String),
    Assistant {
        reasoning: Option<assistant::Reasoning>,
        content: String,
        show_reasoning: bool,
    },
}

impl Item {
    pub fn view(&self, index: usize) -> Element<Message> {
        use iced::border;

        let message: Element<_> = match self {
            Self::Assistant {
                reasoning,
                content,
                show_reasoning,
            } => {
                let reasoning: Element<_> = if let Some(reasoning) = reasoning {
                    let toggle = button(
                        row![
                            text!("Thought for {} seconds", reasoning.duration.as_secs())
                                .font(Font::MONOSPACE)
                                .size(12),
                            if *show_reasoning {
                                icon::arrow_down()
                            } else {
                                icon::arrow_up()
                            }
                            .size(12),
                        ]
                        .spacing(10),
                    )
                    .on_press(Message::ToggleReasoning(index))
                    .style(button::secondary);

                    let thoughts = row![
                        vertical_rule(1),
                        text(&reasoning.content)
                            .size(12)
                            .shaping(text::Shaping::Advanced)
                            .style(|theme: &Theme| {
                                let palette = theme.extended_palette();

                                text::Style {
                                    color: Some(palette.secondary.strong.color),
                                }
                            })
                    ]
                    .spacing(5)
                    .height(Shrink);

                    if *show_reasoning || content.is_empty() {
                        column![toggle, thoughts].spacing(10).into()
                    } else {
                        toggle.into()
                    }
                } else {
                    horizontal_space().into()
                };

                let content = text(content).shaping(text::Shaping::Advanced);

                column![reasoning, content].spacing(10).into()
            }
            Self::User(content) => right(
                container(text(content).shaping(text::Shaping::Advanced))
                    .style(|theme: &Theme| {
                        let palette = theme.extended_palette();

                        container::Style {
                            background: Some(palette.background.weak.color.into()),
                            text_color: Some(palette.background.weak.text),
                            border: border::rounded(10),
                            ..container::Style::default()
                        }
                    })
                    .padding(10),
            )
            .into(),
        };

        let copy = tip(
            button(icon::clipboard())
                .on_press_with(|| Message::Copy(self.clone()))
                .padding(0)
                .style(button::text),
            "Copy to clipboard",
            tip::Position::Bottom,
        );

        hover(
            message,
            container(copy)
                .width(Fill)
                .center_y(Fill)
                .align_x(match self {
                    Self::Assistant { .. } => Right,
                    Self::User(_) => Left,
                }),
        )
    }

    pub fn into_text(self) -> String {
        match self {
            Self::User(content) => content,
            Self::Assistant {
                reasoning,
                content,
                show_reasoning,
            } => match reasoning {
                Some(reasoning) if show_reasoning => {
                    format!("Reasoning:\n{}\n\n{content}", reasoning.content)
                }
                _ => content,
            },
        }
    }
}
