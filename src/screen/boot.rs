use crate::assistant::{Assistant, Backend, BootEvent, Error, File, Model};

use iced::system;
use iced::task::{self, Task};
use iced::time::{self, Duration, Instant};
use iced::widget::{
    button, center, column, container, progress_bar, row, scrollable, stack, text, toggler,
    tooltip, value,
};
use iced::{Alignment, Border, Element, Font, Length, Padding, Subscription, Theme};

pub struct Boot {
    model: Model,
    state: State,
    use_cuda: bool,
}

enum State {
    Idle,
    Booting {
        logs: Vec<String>,
        error: Option<Error>,
        stage: String,
        progress: u64,
        tick: usize,
        task: task::Handle,
    },
}

#[derive(Debug, Clone)]
pub enum Message {
    Boot(File),
    Booting(Result<BootEvent, Error>),
    Tick(Instant),
    Cancel,
    Abort,
    UseCUDAToggled(bool),
}

pub enum Event {
    None,
    Finished(Assistant),
    Aborted,
}

impl Boot {
    pub fn new(model: Model, system: Option<&system::Information>) -> Self {
        let use_cuda = system
            .map(|system| system.graphics_adapter.contains("NVIDIA"))
            .unwrap_or_default();

        Self {
            model: model.clone(),
            state: State::Idle,
            use_cuda,
        }
    }

    pub fn title(&self) -> String {
        match &self.state {
            State::Idle => format!("Booting {name} - Icebreaker", name = self.model.name()),
            State::Booting { progress, .. } => format!(
                "{progress}% - Booting {name} - Icebreaker",
                name = self.model.name()
            ),
        }
    }

    pub fn update(&mut self, message: Message) -> (Task<Message>, Event) {
        match message {
            Message::Boot(file) => {
                let (task, handle) = Task::run(
                    Assistant::boot(
                        file,
                        if self.use_cuda {
                            Backend::CUDA
                        } else {
                            Backend::CPU
                        },
                    ),
                    Message::Booting,
                )
                .abortable();

                self.state = State::Booting {
                    logs: Vec::new(),
                    error: None,
                    stage: "Loading...".to_owned(),
                    progress: 0,
                    tick: 0,
                    task: handle,
                };

                (task, Event::None)
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
                        *stage = new_stage.to_owned();
                        *progress = percent;
                    }

                    (Task::none(), Event::None)
                }
                BootEvent::Logged(log) => {
                    if let State::Booting { logs, .. } = &mut self.state {
                        logs.push(log);
                    }

                    (Task::none(), Event::None)
                }
                BootEvent::Finished(assistant) => (Task::none(), Event::Finished(assistant)),
            },
            Message::Booting(Err(new_error)) => {
                if let State::Booting { error, .. } = &mut self.state {
                    *error = Some(new_error);
                }

                (Task::none(), Event::None)
            }
            Message::Tick(_now) => {
                if let State::Booting { tick, .. } = &mut self.state {
                    *tick += 1;
                }

                (Task::none(), Event::None)
            }
            Message::Cancel => {
                if let State::Booting { task, .. } = &self.state {
                    task.abort();
                }

                self.state = State::Idle;

                (Task::none(), Event::None)
            }
            Message::Abort => (Task::none(), Event::Aborted),
            Message::UseCUDAToggled(use_cuda) => {
                self.use_cuda = use_cuda;

                (Task::none(), Event::None)
            }
        }
    }

    pub fn view(&self) -> Element<Message> {
        let title = {
            text!("Booting {name}...", name = self.model.name(),)
                .size(20)
                .font(Font::MONOSPACE)
        };

        let state: Element<_> = match &self.state {
            State::Idle => {
                let use_cuda = {
                    let toggle = toggler(
                        Some("Use CUDA".to_owned()),
                        self.use_cuda,
                        Message::UseCUDAToggled,
                    )
                    .width(Length::Shrink);

                    tooltip(
                        toggle,
                        container(text("Only supported on NVIDIA cards!").size(12))
                            .padding(5)
                            .style(container::rounded_box),
                        tooltip::Position::Left,
                    )
                };

                let abort = action("Abort")
                    .style(button::danger)
                    .on_press(Message::Abort);

                column![
                    row![
                        text("Select a file to boot:").width(Length::Fill),
                        use_cuda,
                        abort
                    ]
                    .spacing(10)
                    .align_items(Alignment::Center),
                    scrollable(
                        column(self.model.files.iter().map(|file| {
                            button(text(&file.name).font(Font::MONOSPACE))
                                .on_press(Message::Boot(file.clone()))
                                .width(Length::Fill)
                                .padding(10)
                                .style(button::secondary)
                                .into()
                        }))
                        .spacing(10)
                        .padding(Padding::right(10))
                    )
                    .embed_y(true)
                ]
                .spacing(10)
                .into()
            }

            State::Booting {
                logs,
                error,
                stage,
                progress,
                tick,
                ..
            } => {
                let progress = {
                    let stage = if error.is_none() {
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
                    .font(Font::MONOSPACE);

                    let bar = progress_bar(0.0..=100.0, *progress as f32).height(Length::Fill);

                    let cancel = if error.is_none() {
                        action("Cancel").style(button::danger)
                    } else {
                        action("Back").style(button::secondary)
                    }
                    .on_press(Message::Cancel);

                    row![
                        stack![
                            if error.is_none() {
                                bar
                            } else {
                                bar.style(progress_bar::danger)
                            },
                            center(stage.style(|theme: &Theme| text::Style {
                                color: Some(theme.palette().background)
                            }))
                        ],
                        cancel
                    ]
                    .height(Length::Shrink)
                    .spacing(10)
                };

                let error = error
                    .as_ref()
                    .map(|error| value(error).font(Font::MONOSPACE).style(text::danger));

                let logs = scrollable(
                    column(
                        logs.iter()
                            .map(|log| text(log).size(12).font(Font::MONOSPACE).into()),
                    )
                    .push_maybe(error)
                    .spacing(5)
                    .padding(Padding {
                        right: 20.0,
                        ..Padding::ZERO
                    }),
                )
                .align_y(scrollable::Alignment::End)
                .width(Length::Fill)
                .height(Length::Fill);

                column![progress, logs].spacing(10).into()
            }
        };

        let frame = container(state)
            .padding(10)
            .style(|theme: &Theme| container::Style {
                border: Border::rounded(2)
                    .with_width(1)
                    .with_color(theme.palette().text),
                ..container::Style::default()
            })
            .width(800)
            .height(600);

        center(
            column![title, frame]
                .spacing(10)
                .align_items(Alignment::Center),
        )
        .padding(10)
        .into()
    }

    pub fn subscription(&self) -> Subscription<Message> {
        if let State::Booting { error: None, .. } = &self.state {
            time::every(Duration::from_millis(100)).map(Message::Tick)
        } else {
            Subscription::none()
        }
    }
}

fn action(text: &str) -> button::Button<Message> {
    button(container(text).center_x(Length::Fill)).width(70)
}
