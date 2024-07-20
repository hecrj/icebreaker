use crate::assistant::{Backend, File, Model};

use iced::border;
use iced::system;
use iced::widget::{button, center, column, container, row, scrollable, text, toggler, tooltip};
use iced::{Center, Element, Fill, Font, Theme};

pub struct Boot {
    model: Model,
    use_cuda: bool,
}

#[derive(Debug, Clone)]
pub enum Message {
    Boot(File),
    Abort,
    UseCUDAToggled(bool),
}

pub enum Action {
    None,
    Boot { file: File, backend: Backend },
    Abort,
}

impl Boot {
    pub fn new(model: Model, system: Option<&system::Information>) -> Self {
        let use_cuda = system
            .map(|system| system.graphics_adapter.contains("NVIDIA"))
            .unwrap_or_default();

        Self {
            model: model.clone(),
            use_cuda,
        }
    }

    pub fn title(&self) -> String {
        format!("{name} - Icebreaker", name = self.model.name())
    }

    pub fn update(&mut self, message: Message) -> Action {
        match message {
            Message::Boot(file) => Action::Boot {
                file,
                backend: if self.use_cuda {
                    Backend::Cuda
                } else {
                    Backend::Cpu
                },
            },
            Message::Abort => Action::Abort,
            Message::UseCUDAToggled(use_cuda) => {
                self.use_cuda = use_cuda;

                Action::None
            }
        }
    }

    pub fn view(&self) -> Element<Message> {
        let title = {
            text!("Booting {name}...", name = self.model.name(),)
                .size(20)
                .font(Font::MONOSPACE)
        };

        let state = {
            let use_cuda = {
                let toggle = toggler(
                    Some("Use CUDA".to_owned()),
                    self.use_cuda,
                    Message::UseCUDAToggled,
                );

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
                row![text("Select a file to boot:").width(Fill), use_cuda, abort]
                    .spacing(10)
                    .align_y(Center),
                scrollable(
                    column(self.model.files.iter().map(|file| {
                        button(text(&file.name).font(Font::MONOSPACE))
                            .on_press(Message::Boot(file.clone()))
                            .width(Fill)
                            .padding(10)
                            .style(button::secondary)
                            .into()
                    }))
                    .spacing(10)
                )
                .spacing(10)
            ]
            .spacing(10)
        };

        let frame = container(state)
            .padding(10)
            .style(|theme: &Theme| container::Style {
                border: border::rounded(2).width(1).color(theme.palette().text),
                ..container::Style::default()
            })
            .width(800)
            .height(600);

        center(column![title, frame].spacing(10).align_x(Center))
            .padding(10)
            .into()
    }
}

fn action(text: &str) -> button::Button<Message> {
    button(container(text).center_x(Fill)).width(70)
}
