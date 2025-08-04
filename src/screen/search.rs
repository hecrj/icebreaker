use crate::core::model;
use crate::core::{Error, Model};
use crate::icon;
use crate::widget::sidebar_section;

use iced::border;
use iced::font;
use iced::time::Duration;
use iced::widget::{
    self, button, center, center_x, column, container, grid, horizontal_rule, horizontal_space,
    right, row, rule, scrollable, text, text_input, value,
};
use iced::{Center, Element, Fill, Font, Right, Shrink, Task, Theme};
use iced_palace::widget::ellipsized_text;

use function::Binary;

pub struct Search {
    models: Vec<Model>,
    search: String,
    search_temperature: usize,
    is_searching: bool,
    mode: Mode,
}

#[derive(Debug, Clone)]
pub enum Message {
    ModelsListed(Result<Vec<Model>, Error>),
    SearchChanged(String),
    SearchCooled,
    Select(model::Id),
    DetailsFetched(model::Id, Result<model::Details, Error>),
    FilesListed(model::Id, Result<model::Files, Error>),
    Boot(model::File),
    Back,
}

pub enum Mode {
    Search,
    Details {
        model: model::Id,
        details: Option<model::Details>,
        files: Option<model::Files>,
    },
}

pub enum Action {
    None,
    Boot(model::File),
    Run(Task<Message>),
}

impl Search {
    pub fn new() -> (Self, Task<Message>) {
        (
            Self {
                models: Vec::new(),
                search: String::new(),
                search_temperature: 0,
                is_searching: true,
                mode: Mode::Search,
            },
            Task::batch([
                Task::perform(Model::list(), Message::ModelsListed),
                widget::focus_next(),
            ]),
        )
    }

    pub fn title(&self) -> String {
        match &self.mode {
            Mode::Search => "Models - Icebreaker".to_owned(),
            Mode::Details { model, .. } => format!("{} - Icebreaker", model.name()),
        }
    }

    pub fn update(&mut self, message: Message) -> Action {
        match message {
            Message::ModelsListed(Ok(models)) => {
                self.models = models;
                self.is_searching = false;

                Action::None
            }
            Message::ModelsListed(Err(error)) => {
                log::error!("{error}");

                Action::None
            }
            Message::SearchChanged(search) => {
                self.search = search;
                self.search_temperature += 1;

                Action::Run(Task::perform(
                    tokio::time::sleep(Duration::from_secs(1)),
                    |_| Message::SearchCooled,
                ))
            }
            Message::SearchCooled => {
                self.search_temperature = self.search_temperature.saturating_sub(1);

                if self.search_temperature == 0 {
                    self.is_searching = true;

                    Action::Run(Task::perform(
                        Model::search(self.search.clone()),
                        Message::ModelsListed,
                    ))
                } else {
                    Action::None
                }
            }
            Message::Select(model) => {
                self.mode = Mode::Details {
                    model: model.clone(),
                    details: None,
                    files: None,
                };

                Action::Run(Task::batch([
                    Task::perform(
                        model::Details::fetch(model.clone()),
                        Message::DetailsFetched.with(model.clone()),
                    ),
                    Task::perform(
                        model::File::list(model.clone()),
                        Message::FilesListed.with(model),
                    ),
                ]))
            }
            Message::DetailsFetched(new_model, Ok(new_details)) => {
                match &mut self.mode {
                    Mode::Details { model, details, .. } if model == &new_model => {
                        *details = Some(new_details);
                    }
                    _ => {}
                }

                Action::None
            }
            Message::FilesListed(new_model, Ok(new_files)) => {
                match &mut self.mode {
                    Mode::Details { model, files, .. } if model == &new_model => {
                        *files = Some(new_files);
                    }
                    _ => {}
                }

                Action::None
            }
            Message::Back => {
                self.mode = Mode::Search;

                Action::Run(widget::focus_next())
            }
            Message::Boot(file) => Action::Boot(file),
            Message::DetailsFetched(_, Err(error)) | Message::FilesListed(_, Err(error)) => {
                log::error!("{error}");

                Action::None
            }
        }
    }

    pub fn view<'a>(&'a self, library: &'a model::Library) -> Element<'a, Message> {
        match &self.mode {
            Mode::Search => self.search(),
            Mode::Details {
                model,
                details,
                files,
            } => self.details(model, details.as_ref(), files.as_ref(), library),
        }
    }

    pub fn search(&self) -> Element<'_, Message> {
        let search = text_input("Search language models...", &self.search)
            .size(20)
            .padding(10)
            .on_input(Message::SearchChanged);

        let models: Element<'_, _> = {
            let search_terms: Vec<_> = self
                .search
                .trim()
                .split(' ')
                .map(str::to_lowercase)
                .collect();

            let mut filtered_models = self
                .models
                .iter()
                .filter(|model| {
                    self.search.is_empty()
                        || search_terms.iter().all(|term| {
                            model.id.name().to_lowercase().contains(term)
                                || model.id.author().to_lowercase().contains(term)
                        })
                })
                .peekable();

            if filtered_models.peek().is_none() {
                center(text(if self.is_searching || self.search_temperature > 0 {
                    "Searching..."
                } else {
                    "No models found!"
                }))
                .into()
            } else {
                let cards = grid(filtered_models.map(model_card))
                    .spacing(10)
                    .fluid(650)
                    .height(Shrink);

                scrollable(cards).height(Fill).spacing(10).into()
            }
        };

        column![search, models].spacing(10).into()
    }

    pub fn details<'a>(
        &self,
        model: &'a model::Id,
        details: Option<&'a model::Details>,
        files: Option<&'a model::Files>,
        library: &'a model::Library,
    ) -> Element<'a, Message> {
        use iced::widget::Text;

        let back = button(row![icon::left(), "All models"].align_y(Center).spacing(10))
            .padding([10, 0])
            .on_press(Message::Back)
            .style(button::text);

        fn badge<'a>(icon: Text<'a>, value: Text<'a>) -> Element<'a, Message> {
            container(
                row![
                    icon.size(10)
                        .style(|theme| text::Style {
                            color: Some(theme.extended_palette().background.strongest.color)
                        })
                        .line_height(1.0),
                    value.size(12).font(Font::MONOSPACE)
                ]
                .align_y(Center)
                .spacing(5),
            )
            .padding([4, 7])
            .style(container::bordered_box)
            .into()
        }

        let header = {
            let title = center_x(
                row![
                    text(model.author()).size(18),
                    text("/").size(18),
                    ellipsized_text(model.name())
                        .size(20)
                        .font(Font {
                            weight: font::Weight::Semibold,
                            ..Font::MONOSPACE
                        })
                        .wrapping(text::Wrapping::None)
                ]
                .align_y(Center)
                .spacing(5),
            );

            let badges = details.map(|details| {
                row![
                    badge(icon::sliders(), value(details.parameters)),
                    details
                        .architecture
                        .as_ref()
                        .map(|architecture| badge(icon::server(), text(architecture))),
                    badge(icon::star(), value(details.likes)),
                    badge(icon::download(), value(details.downloads)),
                    badge(
                        icon::clock(),
                        value(details.last_modified.format("%-e %B, %Y")),
                    ),
                ]
                .align_y(Center)
                .spacing(10)
            });

            column![title, badges].spacing(10).align_x(Center)
        };

        let download = files.map(|files| view_files(files, library));

        scrollable(center_x(
            column![back, header, download]
                .spacing(20)
                .max_width(600)
                .clip(true),
        ))
        .spacing(10)
        .into()
    }

    pub fn sidebar<'a>(&'a self, library: &'a model::Library) -> Element<'a, Message> {
        let header = sidebar_section("Models", icon::search(), Message::Back);

        if library.files().is_empty() {
            return column![
                header,
                center(
                    text("No models have been downloaded yet.\n\nFind some to start chatting →")
                        .width(Fill)
                        .center()
                        .shaping(text::Shaping::Advanced)
                )
            ]
            .spacing(10)
            .into();
        }

        let library = column(library.files().iter().map(|file| {
            let title = ellipsized_text(file.model.name())
                .font(Font::MONOSPACE)
                .wrapping(text::Wrapping::None);

            let author = row![
                icon::user()
                    .size(10)
                    .line_height(1.0)
                    .style(text::secondary),
                text(file.model.author()).size(12).style(text::secondary),
            ]
            .spacing(5)
            .align_y(Center);

            let variant = file.variant().map(|variant| {
                text(variant)
                    .font(Font::MONOSPACE)
                    .size(12)
                    .style(text::secondary)
            });

            let entry = column![
                title,
                row![author, horizontal_space(), variant]
                    .spacing(5)
                    .align_y(Center)
            ]
            .spacing(2);

            let is_active = match &self.mode {
                Mode::Details { model, .. } => model == &file.model,
                _ => false,
            };

            button(entry)
                .on_press_with(|| Message::Select(file.model.clone()))
                .padding([8, 10])
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
        }));

        column![header, scrollable(library).spacing(10).height(Fill)]
            .spacing(10)
            .into()
    }
}

fn model_card(model: &Model) -> Element<'_, Message> {
    use iced::widget::Text;

    fn stat<'a>(
        icon: Text<'a>,
        value: Text<'a>,
        style: fn(&Theme) -> text::Style,
    ) -> Element<'a, Message> {
        row![
            icon.size(10).line_height(1.0).style(style),
            value.size(12).font(Font::MONOSPACE).style(style)
        ]
        .align_y(Center)
        .spacing(5)
        .into()
    }

    let title = ellipsized_text(model.id.name())
        .font(Font::MONOSPACE)
        .wrapping(text::Wrapping::None);

    let metadata = row![
        stat(icon::user(), text(model.id.author()), text::secondary),
        stat(
            icon::clock(),
            value(model.last_modified.format("%-e %B, %y")),
            text::secondary,
        ),
        stat(icon::download(), value(model.downloads), text::primary),
        stat(icon::star(), value(model.likes), text::warning),
    ]
    .spacing(20);

    button(column![title, metadata].spacing(10))
        .width(Fill)
        .padding(10)
        .style(|theme, status| {
            let palette = theme.extended_palette();

            let base = button::Style {
                background: Some(palette.background.weakest.color.into()),
                text_color: palette.background.weakest.text,
                border: border::rounded(2)
                    .color(palette.background.weak.color)
                    .width(1),
                ..button::Style::default()
            };

            match status {
                button::Status::Active | button::Status::Disabled => base,
                button::Status::Hovered => button::Style {
                    background: Some(palette.background.weak.color.into()),
                    text_color: palette.background.weak.text,
                    border: base.border.color(palette.background.strong.color),
                    ..base
                },
                button::Status::Pressed => button::Style {
                    border: base.border.color(palette.background.strongest.color),
                    ..base
                },
            }
        })
        .on_press_with(|| Message::Select(model.id.clone()))
        .into()
}

pub fn view_files<'a>(
    files: &'a model::Files,
    library: &'a model::Library,
) -> Element<'a, Message> {
    use itertools::Itertools;

    fn view_file<'a>(
        file: &'a model::File,
        library: &'a model::Library,
    ) -> Option<Element<'a, Message>> {
        let variant = file.variant()?;
        let is_ready = library.files().contains(file);

        Some(
            button(
                row![
                    is_ready.then(|| icon::check().style(text::primary).size(12)),
                    text(variant)
                        .font(Font::MONOSPACE)
                        .size(12)
                        .style(if is_ready {
                            text::primary
                        } else {
                            text::default
                        }),
                    file.size.map(|size| value(size)
                        .font(Font::MONOSPACE)
                        .size(10)
                        .style(text::secondary))
                ]
                .align_y(Center)
                .spacing(5),
            )
            .on_press_with(|| Message::Boot(file.clone()))
            .style(move |theme, status| {
                let base = button::background(theme, status);

                if is_ready {
                    button::Style {
                        border: base.border.color(theme.palette().primary).width(1),
                        ..base
                    }
                } else {
                    base
                }
            })
            .into(),
        )
    }

    let files: Element<'_, _> = if files.is_empty() {
        container(
            text("No compatible files have been found for this model.")
                .width(Fill)
                .center(),
        )
        .padding(20)
        .into()
    } else {
        let files = files.iter().map(|(bit, variants)| {
            row![
                value(bit).font(Font::MONOSPACE).size(14).width(80),
                right(
                    row(variants.iter().filter_map(|file| view_file(file, library)))
                        .spacing(10)
                        .wrap()
                        .align_x(Right)
                ),
            ]
            .align_y(Center)
            .into()
        });

        column(Itertools::intersperse_with(files, || {
            horizontal_rule(1).style(rule::weak).into()
        }))
        .spacing(10)
        .into()
    };

    container(files)
        .padding(10)
        .style(container::bordered_box)
        .into()
}
