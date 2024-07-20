use crate::assistant::{self, Assistant, Backend, BootEvent, ChatEvent, Error, File};
use crate::icon;

use iced::clipboard;
use iced::padding;
use iced::task::{self, Task};
use iced::time::{self, Duration, Instant};
use iced::widget::{
    self, button, center, column, container, hover, progress_bar, scrollable, stack, text,
    text_input, tooltip, value,
};
use iced::{Center, Element, Fill, Font, Left, Right, Subscription, Theme};

pub struct Conversation {
    state: State,
    history: Vec<assistant::Message>,
    input: String,
    error: Option<Error>,
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
        is_sending: bool,
    },
}

#[derive(Debug, Clone)]
pub enum Message {
    Booting(Result<BootEvent, Error>),
    Tick(Instant),
    InputChanged(String),
    InputSubmitted,
    Chatting(Result<ChatEvent, Error>),
    Copy(assistant::Message),
    Back,
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
                state: State::Booting {
                    file,
                    logs: Vec::new(),
                    stage: "Booting...".to_owned(),
                    progress: 0,
                    tick: 0,
                    _task: handle.abort_on_drop(),
                },
                history: Vec::new(),
                input: String::new(),
                error: None,
            },
            Task::batch([boot, widget::focus_next()]),
        )
    }

    pub fn title(&self) -> String {
        format!("{name} - Icebreaker", name = self.model_name())
    }

    pub fn update(&mut self, message: Message) -> Action {
        match message {
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
                        is_sending: false,
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
                if let State::Running {
                    assistant,
                    is_sending,
                } = &mut self.state
                {
                    *is_sending = true;

                    Action::Run(Task::run(
                        assistant.chat(&self.history, &self.input),
                        Message::Chatting,
                    ))
                } else {
                    Action::None
                }
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
                        if let State::Running { is_sending, .. } = &mut self.state {
                            *is_sending = false;
                        }
                    }
                }

                Action::None
            }
            Message::Chatting(Err(error)) => {
                self.error = Some(dbg!(error));

                if let State::Running { is_sending, .. } = &mut self.state {
                    *is_sending = false;
                }

                Action::None
            }
            Message::Copy(message) => Action::Run(clipboard::write(message.content().to_owned())),
            Message::Back => Action::Back,
        }
    }

    pub fn view(&self) -> Element<Message> {
        let header: Element<_> = {
            let title = text(self.model_name())
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

            let bar = hover(title, container(back).center_y(Fill));

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
                            .height(Fill);

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
                        container(logs).padding(10).style(container::rounded_box),
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

        column![header, messages, input]
            .spacing(10)
            .padding(10)
            .align_x(Center)
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
        matches!(
            self.state,
            State::Running {
                is_sending: false,
                ..
            }
        )
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
}
