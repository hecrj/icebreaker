use crate::core;
use crate::core::assistant::{self, Assistant, Backend, BootEvent, Token};
use crate::core::chat::{self, Chat, Entry, Id, Strategy};
use crate::core::model::File;
use crate::core::plan;
use crate::core::{Error, Url};
use crate::icon;
use crate::widget::tip;

use iced::border;
use iced::clipboard;
use iced::padding;
use iced::task::{self, Task};
use iced::theme::palette;
use iced::time::{self, Duration, Instant};
use iced::widget::{
    self, bottom, bottom_center, button, center, center_x, center_y, column, container,
    horizontal_space, hover, markdown, pop, progress_bar, right, right_center, row, scrollable,
    stack, text, text_editor, tooltip, value, vertical_rule, vertical_space, Text,
};
use iced::{Center, Element, Fill, Font, Shrink, Size, Subscription, Theme};

pub struct Conversation {
    backend: Backend,
    chats: Vec<Entry>,
    state: State,
    id: Option<Id>,
    title: Option<String>,
    history: History,
    input: text_editor::Content,
    input_height: f32,
    strategy: Strategy,
    error: Option<Error>,
    sidebar_open: bool,
}

enum State {
    Booting {
        file: File,
        logs: Vec<String>,
        stage: String,
        progress: u32,
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
    Booting(BootEvent),
    Booted(Result<Assistant, Error>),
    Tick(Instant),
    InputChanged(text_editor::Action),
    InputResized(Size),
    ToggleSearch,
    Submit,
    Chatting(chat::Event),
    Chatted(Result<(), Error>),
    TitleChanging(String),
    TitleChanged(Result<String, Error>),
    Copy(String),
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
    LinkClicked(markdown::Url),
}

pub enum Action {
    None,
    Run(Task<Message>),
    Back,
}

impl Conversation {
    pub fn new(file: File, backend: Backend) -> (Self, Task<Message>) {
        let (boot, handle) = Task::sip(
            Assistant::boot(file.clone(), backend),
            Message::Booting,
            Message::Booted,
        )
        .abortable();

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
                strategy: Strategy::default(),
                error: None,
                chats: Vec::new(),
                sidebar_open: true,
            },
            Task::batch([
                boot,
                Task::perform(Chat::list(), Message::ChatsListed),
                widget::focus_next(),
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
            Message::Booting(event) => match event {
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
            },
            Message::Booted(Ok(assistant)) => {
                self.state = State::Running {
                    assistant,
                    sending: None,
                };

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
            Message::InputResized(bounds) => {
                self.input_height = bounds.height;

                Action::None
            }
            Message::ToggleSearch => {
                self.strategy.search = !self.strategy.search;

                Action::None
            }
            Message::Submit => {
                let State::Running { assistant, sending } = &mut self.state else {
                    return Action::None;
                };

                let content = self.input.text();
                let content = content.trim();

                if content.is_empty() {
                    return Action::None;
                }

                self.input = text_editor::Content::new();
                self.history.push(Item::User {
                    content: Content::parse(content.to_owned()),
                });

                let (send, handle) = Task::sip(
                    chat::complete(assistant, &self.history.to_data(), self.strategy),
                    Message::Chatting,
                    Message::Chatted,
                )
                .abortable();

                *sending = Some(handle.abort_on_drop());

                Action::Run(Task::batch([send, snap_chat_to_end()]))
            }
            Message::TitleChanging(title) => {
                self.title = Some(title);
                Action::None
            }
            Message::TitleChanged(Ok(title)) => {
                self.title = Some(title);
                self.save()
            }
            Message::Chatting(event) if !self.can_send() => match event {
                chat::Event::ReplyAdded => {
                    self.history.push(Item::Reply(Reply::default()));

                    Action::Run(snap_chat_to_end())
                }
                chat::Event::ReplyChanged {
                    reply: new_reply,
                    new_token,
                } => {
                    if let Some(Item::Reply(reply)) = self.history.last_mut() {
                        reply.update(new_reply);
                        reply.push(new_token);
                    }

                    Action::None
                }
                chat::Event::PlanAdded => {
                    self.history.push(Item::Plan(Plan::default()));

                    Action::None
                }
                chat::Event::PlanChanged(event) => {
                    if let Some(Item::Plan(plan)) = self.history.last_mut() {
                        plan.update(event);
                    }

                    Action::None
                }
            },
            Message::Chatting(_outdated_event) => Action::None,
            Message::Chatted(Ok(())) => {
                if let State::Running {
                    sending, assistant, ..
                } = &mut self.state
                {
                    *sending = None;

                    let messages: Vec<_> = self.history.to_data();

                    if self.title.is_none() || messages.len() == 5 {
                        Action::Run(Task::sip(
                            chat::title(&assistant, &messages),
                            Message::TitleChanging,
                            Message::TitleChanged,
                        ))
                    } else {
                        self.save()
                    }
                } else {
                    Action::None
                }
            }
            Message::Chatted(Err(error)) => {
                self.error = Some(dbg!(error));

                if let State::Running { sending, .. } = &mut self.state {
                    *sending = None;
                }

                Action::None
            }
            Message::Copy(content) => Action::Run(clipboard::write(content)),
            Message::Regenerate(index) => {
                let State::Running { assistant, sending } = &mut self.state else {
                    return Action::None;
                };

                self.history.truncate(index);

                let (send, handle) = Task::run(
                    chat::complete(assistant, &self.history.to_data(), self.strategy),
                    Message::Chatting,
                )
                .abortable();

                *sending = Some(handle.abort_on_drop());

                Action::Run(send)
            }
            Message::ToggleReasoning(index) => {
                if let Some(Item::Reply(Reply {
                    reasoning: Some(reasoning),
                    ..
                })) = self.history.get_mut(index)
                {
                    reasoning.show = !reasoning.show;
                }

                Action::None
            }
            Message::Created(Ok(chat)) | Message::Saved(Ok(chat)) => {
                self.id = Some(chat.id);

                Action::Run(Task::perform(Chat::list(), Message::ChatsListed))
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

                        Action::Run(Task::batch([widget::focus_next(), snap_chat_to_end()]))
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
                        let (mut conversation, task) = Self::open(chat, self.backend);
                        conversation.input_height = self.input_height;

                        *self = conversation;

                        Action::Run(task)
                    }
                }
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
            Message::LinkClicked(url) => {
                let _ = open::that_in_background(url.to_string());

                Action::None
            }
            Message::Booted(Err(error))
            | Message::Created(Err(error))
            | Message::Saved(Err(error))
            | Message::TitleChanged(Err(error))
            | Message::ChatFetched(Err(error)) => {
                self.error = Some(dbg!(error));

                Action::None
            }
        }
    }

    pub fn save(&self) -> Action {
        let State::Running { assistant, sending } = &self.state else {
            return Action::None;
        };

        if sending.is_some() {
            return Action::None;
        }

        let items = self.history.to_data();

        if let Some(id) = &self.id {
            Action::Run(Task::perform(
                Chat::save(
                    id.clone(),
                    assistant.file().clone(),
                    self.title.clone(),
                    items,
                ),
                Message::Saved,
            ))
        } else {
            Action::Run(Task::perform(
                Chat::create(assistant.file().clone(), self.title.clone(), items),
                Message::Created,
            ))
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

        let input = {
            let editor = text_editor(&self.input)
                .placeholder("Type your message here...")
                .on_action(Message::InputChanged)
                .padding(padding::all(10).bottom(50))
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
                });

            let strategy = {
                let strategy = self.strategy;

                let search = button(
                    row![icon::globe().size(12), text("Search").size(12)]
                        .spacing(8)
                        .height(Fill)
                        .align_y(Center),
                )
                .height(30)
                .on_press(Message::ToggleSearch)
                .style(move |theme: &Theme, status| {
                    if strategy.search {
                        button::Style {
                            border: border::rounded(5),
                            ..button::primary(
                                theme,
                                match status {
                                    button::Status::Active => button::Status::Hovered,
                                    button::Status::Hovered => button::Status::Active,
                                    _ => status,
                                },
                            )
                        }
                    } else {
                        let palette = theme.extended_palette();

                        let base = button::Style {
                            text_color: palette.background.base.text,
                            border: border::rounded(5)
                                .width(1)
                                .color(palette.background.base.text),
                            ..button::Style::default()
                        };

                        match status {
                            button::Status::Active | button::Status::Pressed => base,
                            button::Status::Hovered => button::Style {
                                background: Some(
                                    palette.background.base.text.scale_alpha(0.2).into(),
                                ),
                                ..base
                            },
                            button::Status::Disabled => button::Style::default(),
                        }
                    }
                });

                bottom(search).padding(10)
            };

            container(stack![editor, strategy])
                .width(Shrink)
                .max_width(600)
        };

        let chat = stack![
            column![header, messages].spacing(10).align_x(Center),
            bottom_center(
                pop(input)
                    .on_show(Message::InputResized)
                    .on_resize(Message::InputResized)
            ),
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

pub struct History {
    items: Vec<Item>,
}

impl History {
    pub fn new() -> Self {
        Self { items: Vec::new() }
    }

    pub fn restore(items: impl IntoIterator<Item = chat::Item>) -> Self {
        Self {
            items: items.into_iter().map(Item::from_data).collect(),
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

    pub fn last_mut(&mut self) -> Option<&mut Item> {
        self.items.last_mut()
    }

    pub fn truncate(&mut self, amount: usize) {
        self.items.truncate(amount);
    }

    pub fn to_data<'a>(&'a self) -> Vec<chat::Item> {
        // TODO: Cache
        self.items.iter().map(Item::to_data).collect()
    }
}

#[derive(Debug)]
pub enum Item {
    User { content: Content },
    Reply(Reply),
    Plan(Plan),
}

#[derive(Debug, Default)]
pub struct Reply {
    reasoning: Option<Reasoning>,
    content: Content,
}

impl Reply {
    fn from_data(reply: assistant::Reply) -> Self {
        Self {
            reasoning: reply.reasoning.map(Reasoning::from_data),
            content: Content::parse(reply.content),
        }
    }

    fn to_data(&self) -> assistant::Reply {
        assistant::Reply {
            reasoning: self.reasoning.as_ref().map(Reasoning::to_data),
            content: self.content.raw.clone(),
        }
    }

    fn to_text(&self) -> String {
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
                    content = self.content.raw
                )
            }
            _ => self.content.raw.clone(),
        }
    }

    fn update(&mut self, new_reply: assistant::Reply) {
        self.reasoning = new_reply.reasoning.map(Reasoning::from_data);
        self.content.raw = new_reply.content;
    }

    fn push(&mut self, new_token: assistant::Token) {
        if let Token::Talking(token) = new_token {
            self.content.markdown.push_str(&token);
        }
    }

    fn view(&self, index: usize, theme: &Theme) -> Element<Message> {
        let message = markdown::view_with(self.content.markdown.items(), theme, &MarkdownViewer);

        if let Some(reasoning) = &self.reasoning {
            column![reasoning.view(index), message].spacing(20).into()
        } else {
            message.into()
        }
    }
}

#[derive(Debug, Default)]
pub struct Plan {
    reasoning: Option<Reasoning>,
    steps: Vec<plan::Step>,
    outcomes: Vec<Outcome>,
}

impl Plan {
    fn from_data(plan: core::Plan) -> Self {
        Self {
            reasoning: plan.reasoning.map(Reasoning::from_data),
            steps: plan.steps,
            outcomes: plan
                .execution
                .outcomes
                .into_iter()
                .map(Outcome::from_data)
                .collect(),
        }
    }

    fn to_data(&self) -> core::Plan {
        core::Plan {
            reasoning: self.reasoning.as_ref().map(Reasoning::to_data),
            steps: self.steps.clone(),
            execution: plan::Execution {
                outcomes: self.outcomes.iter().map(Outcome::to_data).collect(),
            },
        }
    }

    fn update(&mut self, event: plan::Event) {
        match event {
            plan::Event::Designing(reasoning) => {
                self.reasoning = Some(Reasoning::from_data(reasoning));
            }
            plan::Event::Designed(plan) => {
                self.reasoning = plan.reasoning.map(Reasoning::from_data);
                self.steps = plan.steps;
            }
            plan::Event::OutcomeAdded(outcome) => {
                self.outcomes.push(Outcome::from_data(outcome));
            }
            plan::Event::OutcomeChanged(new_outcome) => {
                let Some(Outcome::Answer(plan::Status::Active(mut reply))) = self.outcomes.pop()
                else {
                    self.outcomes.push(Outcome::from_data(new_outcome));
                    return;
                };

                let plan::Outcome::Answer(new_status) = new_outcome else {
                    return;
                };

                self.outcomes
                    .push(Outcome::Answer(new_status.map(move |new_reply| {
                        reply.update(new_reply);
                        reply
                    })));
            }
            plan::Event::Understanding(token) => {
                if let Some(Outcome::Answer(plan::Status::Active(reply))) = self.outcomes.last_mut()
                {
                    reply.push(token);
                }
            }
        }
    }

    fn view(&self, index: usize, theme: &Theme) -> Element<Message> {
        let steps: Element<_> = if self.steps.is_empty() {
            text("Designing a plan...")
                .font(Font::MONOSPACE)
                .width(Fill)
                .center()
                .into()
        } else {
            column(
                self.steps
                    .iter()
                    .zip(
                        self.outcomes
                            .iter()
                            .map(Some)
                            .chain(std::iter::repeat(None)),
                    )
                    .enumerate()
                    .map(|(n, (step, outcome))| {
                        let status = outcome.map(Outcome::status).unwrap_or(Status::Pending);

                        let text_style = match status {
                            Status::Pending => text::default,
                            Status::Active => text::primary,
                            Status::Done => text::success,
                            Status::Error => text::danger,
                        };

                        let number = center(
                            text!("{}", n + 1)
                                .size(12)
                                .font(Font::MONOSPACE)
                                .style(text_style),
                        )
                        .width(24)
                        .height(24)
                        .style(move |theme| {
                            let pair = status.color(theme);

                            container::Style::default()
                                .border(border::rounded(8).color(pair.color).width(1))
                        });

                        let title = row![
                            number,
                            text(&step.description)
                                .font(Font::MONOSPACE)
                                .style(text_style)
                        ]
                        .spacing(20)
                        .align_y(Center);

                        let step: Element<_> = if let Some(outcome) = outcome {
                            column![
                                title,
                                container(outcome.view(index, theme)).padding(padding::left(44))
                            ]
                            .spacing(10)
                            .into()
                        } else {
                            title.into()
                        };

                        step
                    }),
            )
            .spacing(30)
            .into()
        };

        if let Some(reasoning) = &self.reasoning {
            column![reasoning.view(index), steps].spacing(30).into()
        } else {
            steps.into()
        }
    }
}

#[derive(Debug)]
pub enum Outcome {
    Search(plan::Status<Vec<Url>>),
    ScrapeText(plan::Status<Vec<String>>),
    Answer(plan::Status<Reply>),
}

impl Outcome {
    pub fn from_data(outcome: plan::Outcome) -> Self {
        match outcome {
            plan::Outcome::Search(status) => Self::Search(status),
            plan::Outcome::ScrapeText(status) => Self::ScrapeText(status.map(|sites| {
                sites
                    .iter()
                    .flat_map(|text| text.lines())
                    .map(str::to_owned)
                    .collect()
            })),
            plan::Outcome::Answer(status) => Self::Answer(status.map(Reply::from_data)),
        }
    }

    pub fn to_data(&self) -> plan::Outcome {
        match self {
            Outcome::Search(status) => plan::Outcome::Search(status.clone()),
            Outcome::ScrapeText(status) => plan::Outcome::ScrapeText(status.clone()),
            Outcome::Answer(status) => plan::Outcome::Answer(status.as_ref().map(Reply::to_data)),
        }
    }

    pub fn view(&self, index: usize, theme: &Theme) -> Element<Message> {
        fn show_status<'a, T>(
            status: &'a plan::Status<T>,
            show: impl Fn(&'a T) -> Element<'a, Message>,
        ) -> Element<'a, Message> {
            status.result().map(show).unwrap_or_else(error)
        }

        fn error(error: &str) -> Element<Message> {
            text(error).style(text::danger).font(Font::MONOSPACE).into()
        }

        fn links(links: &Vec<Url>) -> Element<Message> {
            container(
                container(
                    column(
                        links
                            .iter()
                            .map(|link| text(link.as_str()).size(12).font(Font::MONOSPACE).into()),
                    )
                    .spacing(5),
                )
                .width(Fill)
                .padding(10)
                .style(container::dark),
            )
            .into()
        }

        fn scraped_text(lines: &Vec<String>) -> Element<Message> {
            container(
                container(
                    scrollable(
                        column(
                            lines
                                .iter()
                                .map(|line| text(line).size(12).font(Font::MONOSPACE).into()),
                        )
                        .spacing(5),
                    )
                    .spacing(5),
                )
                .width(Fill)
                .padding(10)
                .max_height(150)
                .style(container::dark),
            )
            .into()
        }

        fn reply<'a>(reply: &'a Reply, index: usize, theme: &Theme) -> Element<'a, Message> {
            reply.view(index, theme)
        }

        match self {
            Outcome::Search(status) => show_status(status, links),
            Outcome::ScrapeText(status) => show_status(status, scraped_text),
            Outcome::Answer(status) => show_status(status, |value| reply(value, index, theme)),
        }
    }

    fn status(&self) -> Status {
        let status = match self {
            Outcome::Search(status) => status.as_ref().map(|_| ()),
            Outcome::ScrapeText(status) => status.as_ref().map(|_| ()),
            Outcome::Answer(status) => status.as_ref().map(|_| ()),
        };

        match status {
            plan::Status::Active(_) => Status::Active,
            plan::Status::Done(_) => Status::Done,
            plan::Status::Errored(_) => Status::Error,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum Status {
    Pending,
    Active,
    Error,
    Done,
}

impl Status {
    fn color(self, theme: &Theme) -> palette::Pair {
        let palette = theme.extended_palette();

        match self {
            Status::Pending => palette.secondary.base,
            Status::Active => palette.primary.base,
            Status::Done => palette.success.base,
            Status::Error => palette.danger.base,
        }
    }
}

impl Item {
    pub fn view<'a>(&'a self, index: usize, theme: &Theme) -> Element<'a, Message> {
        use iced::border;

        match self {
            Self::User { content } => {
                let message = container(
                    container(markdown::view_with(
                        content.markdown.items(),
                        theme,
                        &MarkdownViewer,
                    ))
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

                right(hover(message, center_y(copy(|| self.to_text())))).into()
            }
            Self::Reply(reply) => {
                let actions = {
                    let regenerate = action(icon::refresh(), "Regenerate", move || {
                        Message::Regenerate(index)
                    });

                    row![copy(|| self.to_text()), regenerate].spacing(10)
                };

                hover(
                    container(reply.view(index, theme)).padding([30, 0]),
                    bottom(actions),
                )
            }
            Self::Plan(plan) => {
                let actions = {
                    let regenerate = action(icon::refresh(), "Regenerate", move || {
                        Message::Regenerate(index)
                    });

                    row![copy(|| self.to_text()), regenerate].spacing(10)
                };

                hover(
                    container(plan.view(index, theme)).padding([30, 0]),
                    bottom(actions),
                )
            }
        }
    }

    pub fn to_text(&self) -> String {
        match self {
            Self::User { content } => content.raw.clone(),
            Self::Reply(reply) => reply.to_text(),
            Self::Plan { .. } => {
                // TODO
                "TODO".to_owned()
            }
        }
    }

    fn from_data(item: chat::Item) -> Self {
        match item {
            chat::Item::User(content) => Item::User {
                content: Content::parse(content),
            },
            chat::Item::Reply(reply) => Self::Reply(Reply::from_data(reply)),
            chat::Item::Plan(plan) => Self::Plan(Plan::from_data(plan)),
        }
    }

    fn to_data(&self) -> chat::Item {
        match self {
            Self::User { content, .. } => chat::Item::User(content.raw.clone()),
            Self::Reply(reply) => chat::Item::Reply(reply.to_data()),
            Self::Plan(plan) => chat::Item::Plan(plan.to_data()),
        }
    }
}

#[derive(Debug, Default)]
pub struct Content {
    raw: String,
    markdown: markdown::Content,
}

impl Content {
    pub fn parse(raw: String) -> Self {
        let markdown = markdown::Content::parse(&raw);

        Self { raw, markdown }
    }
}

#[derive(Debug, Clone)]
pub struct Reasoning {
    thoughts: Vec<String>,
    duration: Duration,
    show: bool,
}

impl Reasoning {
    fn from_data(reasoning: assistant::Reasoning) -> Self {
        Self {
            thoughts: reasoning.content.split("\n\n").map(str::to_owned).collect(),
            duration: reasoning.duration,
            show: true,
        }
    }

    fn to_data(&self) -> assistant::Reasoning {
        assistant::Reasoning {
            content: self.thoughts.join("\n\n"),
            duration: self.duration,
        }
    }

    fn view(&self, index: usize) -> Element<'_, Message> {
        let toggle = button(
            row![
                text!(
                    "Thought for {duration} second{plural}",
                    duration = self.duration.as_secs(),
                    plural = if self.duration.as_secs() != 1 {
                        "s"
                    } else {
                        ""
                    }
                )
                .font(Font::MONOSPACE)
                .size(12),
                if self.show {
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

        let reasoning: Element<_> = if self.show {
            let thoughts = column(self.thoughts.iter().map(|thought| {
                text(thought)
                    .size(12)
                    .shaping(text::Shaping::Advanced)
                    .into()
            }))
            .spacing(12);

            column![
                toggle,
                row![vertical_rule(1), thoughts].spacing(10).height(Shrink)
            ]
            .spacing(10)
            .into()
        } else {
            toggle.into()
        };

        reasoning
    }
}

const CHAT: &str = "chat";

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

fn copy<'a>(to_text: impl Fn() -> String + 'a) -> Element<'a, Message> {
    action(icon::clipboard(), "Copy", move || Message::Copy(to_text()))
}

struct MarkdownViewer;

impl<'a> markdown::Viewer<'a, Message> for MarkdownViewer {
    fn on_link_click(url: markdown::Url) -> Message {
        Message::LinkClicked(url)
    }

    fn code_block(
        &self,
        settings: markdown::Settings,
        _language: Option<&'a str>,
        code: &'a str,
        lines: &'a [markdown::Text],
    ) -> Element<'a, Message> {
        let code_block = markdown::code_block(settings, lines, Message::LinkClicked);

        let copy = tip(
            button(icon::clipboard().size(14))
                .padding(2)
                .on_press_with(|| Message::Copy(code.to_owned()))
                .style(button::text),
            "Copy",
            tip::Position::Bottom,
        );

        hover(
            code_block,
            right(container(copy).style(container::dark)).padding(settings.code_size / 2),
        )
    }
}
