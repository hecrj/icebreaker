use crate::assistant::{Assistant, BootEvent, Error, File, Model};

use iced::alignment::{self, Alignment};
use iced::task::{self, Task};
use iced::time::{self, Duration, Instant};
use iced::widget::{
    button, center, column, container, progress_bar, row, scrollable, stack, text, value,
};
use iced::{Border, Element, Font, Length, Padding, Subscription, Theme};

pub struct Boot {
    model: Model,
    state: State,
}

enum State {
    Idle,
    Booting {
        logs: Vec<String>,
        error: Option<Error>,
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
}

pub enum Event {
    None,
    Finished(Assistant),
    Aborted,
}

impl Boot {
    pub fn new(model: Model) -> Self {
        Self {
            model: model.clone(),
            state: State::Idle,
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
                let (task, handle) = Task::run(Assistant::boot(file), Message::Booting).abortable();

                self.state = State::Booting {
                    logs: Vec::new(),
                    error: None,
                    progress: 0,
                    tick: 0,
                    task: handle,
                };

                (task, Event::None)
            }
            Message::Booting(Ok(event)) => match event {
                BootEvent::Progressed { percent } => {
                    if let State::Booting { progress, .. } = &mut self.state {
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
                let abort = container(
                    button("Abort")
                        .style(button::danger)
                        .on_press(Message::Abort),
                )
                .width(Length::Fill)
                .align_x(alignment::Horizontal::Right);

                column![
                    row![text("Select a file to boot:").width(Length::Fill), abort]
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
                    )
                ]
                .spacing(10)
                .into()
            }

            State::Booting {
                logs,
                error,
                progress,
                tick,
                ..
            } => {
                let logs = scrollable(
                    column(
                        logs.iter()
                            .map(|log| text(log).size(12).font(Font::MONOSPACE).into()),
                    )
                    .push(if let Some(error) = error.as_ref() {
                        value(error).font(Font::MONOSPACE).style(text::danger)
                    } else {
                        text(match tick % 4 {
                            0 => "|",
                            1 => "/",
                            2 => "—",
                            _ => "\\",
                        })
                        .size(12)
                        .font(Font::MONOSPACE)
                    })
                    .spacing(5)
                    .padding(Padding {
                        right: 20.0,
                        ..Padding::ZERO
                    }),
                )
                .align_y(scrollable::Alignment::End)
                .width(Length::Fill)
                .height(Length::Fill);

                let cancel = container(
                    if error.is_none() {
                        button("Cancel").style(button::danger)
                    } else {
                        button("Back").style(button::secondary)
                    }
                    .on_press(Message::Cancel),
                )
                .width(Length::Fill)
                .align_x(alignment::Horizontal::Right);

                let progress = progress_bar(0.0..=100.0, *progress as f32).height(20);

                column![
                    stack![logs, cancel],
                    if error.is_none() {
                        progress
                    } else {
                        progress.style(progress_bar::danger)
                    }
                ]
                .spacing(10)
                .into()
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
