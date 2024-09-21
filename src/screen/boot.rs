use crate::data::assistant::{Backend, File, Model};
use crate::widget::tip;

use iced::system;
use iced::widget::{
    button, center, column, container, horizontal_space, markdown, pick_list, rich_text, row,
    scrollable, span, text, toggler,
};
use iced::{Center, Element, Fill, Font, Task, Theme};

pub struct Boot {
    model: Model,
    file: Option<File>,
    readme: Vec<markdown::Item>,
    use_cuda: bool,
}

#[derive(Debug, Clone)]
pub enum Message {
    ReadmeFetched(Vec<markdown::Item>),
    FileSelected(File),
    Boot,
    Abort,
    UseCUDAToggled(bool),
    LinkClicked(markdown::Url),
}

pub enum Action {
    None,
    Boot { file: File, backend: Backend },
    Abort,
}

impl Boot {
    pub fn new(model: Model, system: Option<&system::Information>) -> (Self, Task<Message>) {
        (
            Self {
                model: model.clone(),
                file: if model.files.len() == 1 {
                    model.files.first().cloned()
                } else {
                    None
                },
                readme: Vec::new(),
                use_cuda: system
                    .map(|system| Backend::detect(&system.graphics_adapter) == Backend::Cuda)
                    .unwrap_or_default(),
            },
            Task::future(model.fetch_readme())
                .and_then(|readme| {
                    Task::future(async move {
                        tokio::task::spawn_blocking(move || markdown::parse(&readme).collect())
                            .await
                            .unwrap_or_default()
                    })
                })
                .map(Message::ReadmeFetched),
        )
    }

    pub fn title(&self) -> String {
        format!("{name} - Icebreaker", name = self.model.name())
    }

    pub fn update(&mut self, message: Message) -> Action {
        match message {
            Message::ReadmeFetched(readme) => {
                self.readme = readme;

                Action::None
            }
            Message::FileSelected(file) => {
                self.file = Some(file);

                Action::None
            }
            Message::Boot => {
                if let Some(file) = self.file.clone() {
                    Action::Boot {
                        file,
                        backend: if self.use_cuda {
                            Backend::Cuda
                        } else {
                            Backend::Cpu
                        },
                    }
                } else {
                    Action::None
                }
            }
            Message::Abort => Action::Abort,
            Message::UseCUDAToggled(use_cuda) => {
                self.use_cuda = use_cuda;

                Action::None
            }
            Message::LinkClicked(url) => {
                let _ = open::that_in_background(url.to_string());

                Action::None
            }
        }
    }

    pub fn view(&self, theme: Theme) -> Element<Message> {
        let title = text(self.model.name()).size(20).font(Font::MONOSPACE);

        let boot = {
            let use_cuda = {
                let toggle = toggler(self.use_cuda)
                    .label("Use CUDA")
                    .on_toggle(Message::UseCUDAToggled);

                tip(
                    toggle,
                    "Only supported on NVIDIA cards!",
                    tip::Position::Left,
                )
            };

            let boot = action("Boot")
                .style(button::success)
                .on_press_maybe(self.file.is_some().then_some(Message::Boot));

            let abort = action("Abort")
                .style(button::danger)
                .on_press(Message::Abort);

            let file = pick_list(
                self.model.files.as_slice(),
                self.file.as_ref(),
                Message::FileSelected,
            )
            .width(Fill)
            .placeholder("Select a file to boot...");

            column![
                file,
                row![abort, horizontal_space(), use_cuda, boot]
                    .spacing(10)
                    .align_y(Center)
            ]
            .spacing(10)
        };

        let readme: Element<_> = if self.readme.is_empty() {
            center(rich_text![
                "Loading ",
                span("README").font(Font::MONOSPACE),
                "..."
            ])
            .into()
        } else {
            scrollable(
                markdown(
                    &self.readme,
                    markdown::Settings::default(),
                    markdown::Style::from_palette(theme.palette()),
                )
                .map(Message::LinkClicked),
            )
            .spacing(10)
            .height(Fill)
            .into()
        };

        center(
            column![title, readme, boot]
                .max_width(600)
                .spacing(10)
                .align_x(Center),
        )
        .padding(10)
        .into()
    }
}

fn action(text: &str) -> button::Button<Message> {
    button(container(text).center_x(Fill)).width(100)
}
