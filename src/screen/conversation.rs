use crate::core::assistant::{Assistant, Backend, BootEvent};
use crate::core::chat::{self, Chat, Entry, Id, Strategy};
use crate::core::model::File;
use crate::core::Error;
use crate::icon;
use crate::ui::markdown;
use crate::ui::plan;
use crate::ui::{Markdown, Plan, Reply};
use crate::widget::{copy, regenerate, sidebar_section, tip, toggle};

use iced::border;
use iced::clipboard;
use iced::gradient;
use iced::padding;
use iced::task::{self, Task};
use iced::time::{self, Duration, Instant};
use iced::widget::{
    self, bottom, bottom_right, button, center, center_x, center_y, column, container,
    horizontal_space, hover, opaque, progress_bar, right, right_center, row, scrollable, sensor,
    stack, text, text_editor, tooltip, value, vertical_space,
};
use iced::Degrees;
use iced::{Center, Color, Element, Fill, Font, Function, Shrink, Size, Subscription, Theme};
use iced_palace::widget::ellipsized_text;

pub struct Conversation {
    backend: Backend,
    chats: Vec<Entry>,
    state: State,
    id: Option<Id>,
    title: Option<String>,
    history: History,
    input: text_editor::Content,
    header_height: f32,
    chat_width: f32,
    input_height: f32,
    total_width: f32,
    strategy: Strategy,
    error: Option<Error>,
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
    Resized(Size),
    HeaderShown(Size),
    HeaderResized(Size),
    ChatResized(Size),
    InputResized(Size),
    ToggleSearch,
    Submit,
    Regenerate(usize),
    Chatting(chat::Event),
    Chatted(Result<(), Error>),
    TitleChanging(String),
    TitleChanged(Result<String, Error>),
    Copy(String),
    ToggleReasoning(usize, bool),
    Created(Result<Chat, Error>),
    Saved(Result<Chat, Error>),
    Open(chat::Id),
    ChatFetched(Result<Chat, Error>),
    LastChatFetched(Result<Chat, Error>),
    Delete,
    New,
    Plan(usize, plan::Message),
    Markdown(markdown::Interaction),
}

pub enum Action {
    None,
    Run(Task<Message>),
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
                header_height: 0.0,
                chat_width: 0.0,
                input_height: 0.0,
                total_width: 0.0,
                strategy: Strategy::default(),
                error: None,
                chats: Vec::new(),
            },
            Task::batch([boot, Task::perform(Chat::list(), Message::ChatsListed)]),
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
            Message::Resized(bounds) => {
                self.total_width = bounds.width;

                Action::None
            }
            Message::HeaderShown(bounds) => {
                self.header_height = bounds.height;

                Action::Run(Task::batch([widget::focus_next(), snap_chat_to_end()]))
            }
            Message::HeaderResized(bounds) => {
                self.header_height = bounds.height;

                Action::None
            }
            Message::ChatResized(bounds) => {
                self.chat_width = bounds.width;

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
                    content: content.to_owned(),
                    markdown: Markdown::parse(content),
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
            Message::Regenerate(index) => {
                let State::Running { assistant, sending } = &mut self.state else {
                    return Action::None;
                };

                self.history.truncate(index);

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
                chat::Event::ReplyChanged(new_reply) => {
                    if let Some(Item::Reply(reply)) = self.history.last_mut() {
                        reply.update(new_reply);
                    }

                    Action::None
                }
                chat::Event::PlanAdded => {
                    self.history.push(Item::Plan(Plan::default()));

                    Action::None
                }
                chat::Event::PlanChanged(event) => {
                    if let Some(Item::Plan(plan)) = self.history.last_mut() {
                        plan.apply(event);
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

                    if self.title.is_none() || messages.len() == 2 || messages.len() == 6 {
                        Action::Run(Task::sip(
                            chat::title(assistant, &messages),
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
            Message::ToggleReasoning(index, show) => {
                if let Some(Item::Reply(reply)) = self.history.get_mut(index) {
                    reply.toggle_reasoning(show);
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

                        Action::None
                    }
                    State::Running { assistant, sending } if assistant.file() == &chat.file => {
                        self.id = Some(chat.id);
                        self.title = chat.title;
                        self.history = History::restore(chat.history);
                        self.input = text_editor::Content::new();
                        self.error = None;

                        *sending = None;

                        Action::None
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
                if let Some(id) = self.id {
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
            Message::Plan(index, message) => {
                let Some(Item::Plan(plan)) = self.history.items.get_mut(index) else {
                    return Action::None;
                };

                Action::Run(plan.update(message).map(Message::Plan.with(index)))
            }
            Message::Markdown(interaction) => Action::Run(interaction.perform()),
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
                Chat {
                    id: *id,
                    file: assistant.file().clone(),
                    title: self.title.clone(),
                    history: items,
                }
                .save(),
                Message::Saved,
            ))
        } else {
            Action::Run(Task::perform(
                Chat::create(assistant.file().clone(), self.title.clone(), items),
                Message::Created,
            ))
        }
    }

    pub fn view(&self, theme: &Theme) -> Element<'_, Message> {
        let header: Element<'_, _> = {
            let title: Element<'_, _> = match &self.title {
                Some(title) => column![
                    text(title).size(20).width(Fill).align_x(Center),
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

            let delete: Element<'_, _> = if self.id.is_some() {
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

            let bar = hover(center_x(title).padding([0, 40]), right_center(delete));

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
                            .push(error)
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

        let messages: Element<'_, _> = if self.history.is_empty() {
            center(
                match &self.state {
                    State::Running { .. } => column![
                        text("Your assistant is ready."),
                        text("Break the ice! ↓")
                            .style(text::primary)
                            .shaping(text::Shaping::Advanced)
                    ],
                    State::Booting { .. } => column![
                        text("Your assistant is launching..."),
                        text("You can begin typing while you wait! ↓")
                            .style(text::success)
                            .shaping(text::Shaping::Advanced),
                    ],
                }
                .spacing(10)
                .align_x(Center),
            )
            .into()
        } else {
            scrollable(column![
                sensor(horizontal_space())
                    .key(self.id)
                    .on_resize(Message::ChatResized),
                center_x(
                    column(
                        self.history
                            .items()
                            .enumerate()
                            .map(|(i, item)| item.view(i, theme)),
                    )
                    .padding(padding::all(20).top(0))
                    .max_width(600),
                )
                .padding(padding::top(self.header_height).bottom(self.input_height))
            ])
            .id(CHAT)
            .spacing(10)
            .height(Fill)
            .into()
        };

        let input = {
            let editor = text_editor(&self.input)
                .placeholder("Type your message here...")
                .on_action(Message::InputChanged)
                .padding(padding::all(15).bottom(50))
                .min_height(16.0 * 1.3 * 2.0) // approx. 2 lines with 1.3 line height
                .max_height(16.0 * 1.3 * 20.0) // approx. 20 lines
                .key_binding(|key_press| {
                    let modifiers = key_press.modifiers;

                    match text_editor::Binding::from_key_press(key_press) {
                        Some(text_editor::Binding::Enter) if !modifiers.shift() => {
                            Some(text_editor::Binding::Custom(Message::Submit))
                        }
                        binding => binding,
                    }
                })
                .style(|theme, status| {
                    let style = text_editor::default(theme, status);

                    text_editor::Style {
                        border: style.border.rounded(10),
                        ..style
                    }
                });

            let strategy = {
                let search = tip(
                    toggle(icon::globe(), "Search", self.strategy.search)
                        .on_press(Message::ToggleSearch),
                    "Very Experimental!",
                    tip::Position::Left,
                );

                bottom_right(search).padding(10)
            };

            container(stack![editor, strategy])
                .width(Shrink)
                .max_width(600)
        };

        let header = container(header)
            .padding(padding::bottom(15))
            .style(|theme| container::Style {
                background: Some(
                    gradient::Linear::new(0)
                        .add_stop(0.0, Color::TRANSPARENT)
                        .add_stop(0.1, theme.palette().background.scale_alpha(0.95))
                        .add_stop(1.0, theme.palette().background)
                        .into(),
                ),
                ..container::Style::default()
            });

        let input = column![
            container(horizontal_space())
                .height(10)
                .style(|theme: &Theme| container::Style {
                    background: Some(
                        gradient::Linear::new(Degrees(180.0))
                            .add_stop(0.0, Color::TRANSPARENT)
                            .add_stop(1.0, theme.palette().background)
                            .into(),
                    ),
                    ..container::Style::default()
                }),
            input
        ]
        .align_x(Center);

        stack![
            sensor(messages)
                .key(self.id)
                .on_show(Message::Resized)
                .on_resize(Message::Resized),
            column![
                sensor(opaque(header))
                    .key(self.id)
                    .on_show(Message::HeaderShown)
                    .on_resize(Message::HeaderResized),
                vertical_space(),
                sensor(input)
                    .key(self.id)
                    .on_show(Message::InputResized)
                    .on_resize(Message::InputResized)
            ]
            .padding(padding::right(
                (self.total_width - self.chat_width).clamp(0.0, 20.0)
            ))
        ]
        .into()
    }

    pub fn sidebar(&self) -> Element<'_, Message> {
        let header = sidebar_section("Chats", icon::plus(), Message::New);

        let chats = column(self.chats.iter().map(|chat| {
            let card = match &chat.title {
                Some(title) => ellipsized_text(title),
                None => ellipsized_text(chat.file.model.name()).font(Font::MONOSPACE),
            }
            .wrapping(text::Wrapping::None);

            let is_active = Some(&chat.id) == self.id.as_ref();

            button(card)
                .on_press_with(move || Message::Open(chat.id))
                .padding([8, 10])
                .width(Fill)
                .style(move |theme, status| {
                    let base = button::Style {
                        border: border::rounded(5),
                        ..button::subtle(theme, status)
                    };

                    if is_active && status == button::Status::Active {
                        let background = theme.extended_palette().background.weak;

                        button::Style {
                            background: Some(background.color.into()),
                            text_color: background.text,
                            ..base
                        }
                    } else {
                        base
                    }
                })
                .into()
        }))
        .clip(true);

        column![header, scrollable(chats).height(Fill).spacing(10)]
            .spacing(10)
            .into()
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

    pub fn to_data(&self) -> Vec<chat::Item> {
        // TODO: Cache
        self.items.iter().map(Item::to_data).collect()
    }
}

#[derive(Debug)]
pub enum Item {
    User { content: String, markdown: Markdown },
    Reply(Reply),
    Plan(Plan),
}

impl Item {
    pub fn view<'a>(&'a self, index: usize, theme: &Theme) -> Element<'a, Message> {
        use iced::border;

        match self {
            Self::User { markdown, .. } => {
                let message = container(
                    container(markdown.view(theme).map(Message::Markdown))
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

                right(hover(
                    message,
                    center_y(copy(|| Message::Copy(self.to_text()))),
                ))
                .into()
            }
            Self::Reply(reply) => self.with_actions(
                reply.view(
                    theme,
                    Message::ToggleReasoning.with(index),
                    Message::Markdown,
                ),
                index,
            ),
            Self::Plan(plan) => {
                self.with_actions(plan.view(theme).map(Message::Plan.with(index)), index)
            }
        }
    }

    pub fn with_actions<'a>(
        &'a self,
        base: Element<'a, Message>,
        index: usize,
    ) -> Element<'a, Message> {
        let actions = row![
            copy(|| Message::Copy(self.to_text())),
            regenerate(move || Message::Regenerate(index))
        ]
        .spacing(10);

        hover(container(base).padding([30, 0]), bottom(actions))
    }

    pub fn to_text(&self) -> String {
        match self {
            Self::User { content, .. } => content.clone(),
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
                markdown: Markdown::parse(&content),
                content,
            },
            chat::Item::Reply(reply) => Self::Reply(Reply::from_data(reply)),
            chat::Item::Plan(plan) => Self::Plan(Plan::from_data(plan)),
        }
    }

    fn to_data(&self) -> chat::Item {
        match self {
            Self::User { content, .. } => chat::Item::User(content.clone()),
            Self::Reply(reply) => chat::Item::Reply(reply.to_data()),
            Self::Plan(plan) => chat::Item::Plan(plan.to_data()),
        }
    }
}

const CHAT: &str = "chat";

fn snap_chat_to_end() -> Task<Message> {
    scrollable::snap_to(CHAT, scrollable::RelativeOffset::END)
}
