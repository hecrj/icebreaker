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
    self, bottom, bottom_center, button, center, center_x, center_y, column, container,
    horizontal_space, hover, markdown, progress_bar, right, right_center, row, scrollable, stack,
    text, text_editor, tooltip, value, vertical_rule, vertical_space, Text,
};
use iced::{Center, Element, Fill, Font, Rectangle, Shrink, Subscription, Theme};

pub struct Conversation {
    backend: Backend,
    chats: Vec<Entry>,
    state: State,
    id: Option<Id>,
    title: Option<String>,
    history: History,
    input: text_editor::Content,
    input_height: f32,
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
    InputMeasured(Option<Rectangle>),
    Submit,
    Chatting(Result<chat::Event, Error>),
    Copy(Item),
    Regenerate(usize),
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
    UrlClicked(markdown::Url),
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
                input_height: 50.0,
                error: None,
                chats: Vec::new(),
                sidebar_open: true,
            },
            Task::batch([
                boot,
                Task::perform(Chat::list(), Message::ChatsListed),
                widget::focus_next(),
                measure_input(),
                snap_chat_to_end(),
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
            task,
        )
    }

    pub fn title(&self) -> String {
        format!(
            "{name} - Icebreaker",
            name = self.title.as_deref().unwrap_or(self.model_name())
        )
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

                Action::Run(measure_input())
            }
            Message::InputMeasured(bounds) => {
                if let Some(bounds) = bounds {
                    self.input_height = bounds.height;
                }

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

                        Action::Run(send)
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

                    Action::Run(snap_chat_to_end())
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
            Message::Regenerate(index) => {
                if let State::Running { assistant, sending } = &mut self.state {
                    self.history.truncate(index);

                    let (send, handle) = Task::run(
                        chat::complete(assistant, self.history.messages().collect()),
                        Message::Chatting,
                    )
                    .abortable();

                    *sending = Some(handle.abort_on_drop());

                    Action::Run(send)
                } else {
                    Action::None
                }
            }
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
                            snap_chat_to_end(),
                            measure_input(),
                        ]))
                    }
                    State::Running { assistant, sending } if assistant.file() == &chat.file => {
                        self.id = Some(chat.id);
                        self.title = chat.title;
                        self.history = History::restore(chat.history);
                        self.input = text_editor::Content::new();
                        self.error = None;

                        *sending = None;

                        Action::Run(Task::batch([widget::focus_next(), snap_chat_to_end()]))
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
            Message::UrlClicked(_url) => {
                // TODO
                Action::None
            }
        }
    }

    pub fn view(&self, theme: &Theme) -> Element<Message> {
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

            let bar = stack![
                center_x(title).padding([0, 40]),
                row![toggle_sidebar, horizontal_space(), delete],
            ]
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

                    stack![bar, right_center(progress)].into()
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
                center_x(
                    column(
                        self.history
                            .items()
                            .enumerate()
                            .map(|(i, item)| item.view(i, theme)),
                    )
                    .padding(20)
                    .max_width(600),
                )
                .padding(padding::bottom(self.input_height)),
            )
            .id(CHAT)
            .spacing(10)
            .height(Fill)
            .into()
        };

        let input = container(
            text_editor(&self.input)
                .placeholder("Type your message here...")
                .on_action(Message::InputChanged)
                .padding(10)
                .min_height(51)
                .max_height(16.0 * 1.3 * 20.0) // approx. 20 lines with 1.3 line height
                .key_binding(|key_press| {
                    let modifiers = key_press.modifiers;

                    match text_editor::Binding::from_key_press(key_press) {
                        Some(text_editor::Binding::Enter) if !modifiers.shift() => {
                            Some(text_editor::Binding::Custom(Message::Submit))
                        }
                        binding => binding,
                    }
                }),
        )
        .width(Shrink)
        .max_width(600);

        let chat = stack![
            column![header, messages].spacing(10).align_x(Center),
            bottom_center(container(input).id(INPUT)),
        ];

        if self.sidebar_open {
            let sidebar = {
                let new = button(text("New Chat").width(Fill).align_x(Center))
                    .on_press(Message::New)
                    .style(button::success);

                let search = button(text("Search Models").width(Fill).align_x(Center))
                    .on_press(Message::Search)
                    .style(button::secondary);

                if self.chats.is_empty() {
                    column![vertical_space(), new, search]
                } else {
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

                    column![scrollable(chats).height(Fill).spacing(10), new, search]
                }
                .width(250)
                .spacing(10)
            };

            row![sidebar, chat].spacing(10).padding(10).into()
        } else {
            container(chat).padding(10).into()
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
            assistant::Message::Assistant { reasoning, content } => {
                let content_markdown = markdown::parse(&content).collect();

                Item::Assistant {
                    reasoning,
                    content,
                    content_markdown,
                    show_reasoning: true,
                }
            }
            assistant::Message::User(content) => {
                let markdown = markdown::parse(&content).collect();

                Item::User { content, markdown }
            }
        }
    }
}

impl From<Item> for assistant::Message {
    fn from(item: Item) -> Self {
        match item {
            Item::User { content, .. } => assistant::Message::User(content),
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

    pub fn truncate(&mut self, amount: usize) {
        self.items.truncate(amount);
    }

    pub fn messages<'a>(&'a self) -> impl Iterator<Item = assistant::Message> + 'a {
        self.items.iter().cloned().map(assistant::Message::from)
    }
}

#[derive(Debug, Clone)]
pub enum Item {
    User {
        content: String,
        markdown: Vec<markdown::Item>,
    },
    Assistant {
        reasoning: Option<assistant::Reasoning>,
        content: String,
        content_markdown: Vec<markdown::Item>,
        show_reasoning: bool,
    },
}

impl Item {
    pub fn view<'a>(&'a self, index: usize, theme: &Theme) -> Element<'a, Message> {
        use iced::border;

        let copy = action(icon::clipboard(), "Copy", || Message::Copy(self.clone()));

        match self {
            Self::Assistant {
                reasoning,
                content,
                content_markdown,
                show_reasoning,
                ..
            } => {
                let message = markdown(
                    content_markdown,
                    markdown::Settings::default(),
                    markdown::Style::from_palette(theme.palette()),
                )
                .map(Message::UrlClicked);

                let message: Element<_> = if let Some(reasoning) = reasoning {
                    let toggle = button(
                        row![
                            text!(
                                "Thought for {duration} second{plural}",
                                duration = reasoning.duration.as_secs(),
                                plural = if reasoning.duration.as_secs() != 1 {
                                    "s"
                                } else {
                                    ""
                                }
                            )
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

                    let reasoning: Element<_> = if *show_reasoning || content.is_empty() {
                        let thoughts = text(&reasoning.content)
                            .size(12)
                            .shaping(text::Shaping::Advanced);

                        column![
                            toggle,
                            row![vertical_rule(1), thoughts].spacing(10).height(Shrink)
                        ]
                        .spacing(10)
                        .into()
                    } else {
                        toggle.into()
                    };

                    column![reasoning, message].spacing(20).into()
                } else {
                    message.into()
                };

                let regenerate = action(icon::refresh(), "Regenerate", move || {
                    Message::Regenerate(index)
                });

                let actions = row![copy, regenerate].spacing(10);

                hover(container(message).padding([30, 0]), bottom(actions))
            }
            Self::User {
                markdown: content, ..
            } => {
                let message = container(
                    container(
                        markdown(
                            content,
                            markdown::Settings::default(),
                            markdown::Style::from_palette(theme.palette()),
                        )
                        .map(Message::UrlClicked),
                    )
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
                .padding(padding::all(20).left(30).right(0));

                right(hover(message, center_y(copy))).into()
            }
        }
    }

    pub fn into_text(self) -> String {
        match self {
            Self::User { content, .. } => content,
            Self::Assistant {
                reasoning,
                content,
                show_reasoning,
                ..
            } => match reasoning {
                Some(reasoning) if show_reasoning => {
                    format!("Reasoning:\n{}\n\n{content}", reasoning.content)
                }
                _ => content,
            },
        }
    }
}

const INPUT: &str = "input";
const CHAT: &str = "chat";

fn measure_input() -> Task<Message> {
    container::visible_bounds(INPUT).map(Message::InputMeasured)
}

fn snap_chat_to_end() -> Task<Message> {
    scrollable::snap_to(CHAT, scrollable::RelativeOffset::END)
}

fn action<'a>(
    icon: Text<'a>,
    label: &'a str,
    message: impl Fn() -> Message + 'a,
) -> Element<'a, Message> {
    tip(
        button(icon)
            .on_press_with(message)
            .padding([2, 7])
            .style(button::text),
        label,
        tip::Position::Bottom,
    )
}
