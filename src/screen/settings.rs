use crate::browser;
use crate::icon;
use crate::model;
use crate::widget::sidebar;

use iced::border;
use iced::font;
use iced::padding;
use iced::widget::{
    button, center_x, center_y, column, container, float, grid, hover, right_center, row,
    scrollable, space, stack, svg, text, value, Svg,
};
use iced::{Center, Element, Fill, Font, Shrink, Task, Theme};
use iced_palace::widget::{ellipsized_text, typewriter};

use std::path::PathBuf;
use std::sync::LazyLock;

pub struct Settings {
    section: Section,
    themes: Vec<Theme>,
}

#[derive(Debug, Clone)]
pub enum Message {
    Open(Section),
    ChangeTheme(Theme),
    OpenTechne,
    PickLibraryFolder,
    PickedLibraryFolder(Option<rfd::FileHandle>),
}

pub enum Action {
    None,
    ChangeTheme(Theme),
    ChangeLibraryFolder(PathBuf),
    Run(Task<Message>),
}

impl Settings {
    pub fn new() -> (Self, Task<Message>) {
        use itertools::Itertools;

        (
            Self {
                section: Section::Storage,
                themes: Theme::ALL
                    .iter()
                    .sorted_by_key(|theme| {
                        (theme.palette().background.relative_luminance() * 1_000.0) as u32
                    })
                    .rev()
                    .cloned()
                    .collect(),
            },
            Task::none(),
        )
    }

    pub fn title(&self) -> &str {
        "Settings"
    }

    pub fn update(&mut self, message: Message) -> Action {
        match message {
            Message::Open(section) => {
                self.section = section;

                Action::None
            }
            Message::ChangeTheme(theme) => Action::ChangeTheme(theme),
            Message::OpenTechne => {
                browser::open("https://github.com/hecrj/techne");

                Action::None
            }
            Message::PickLibraryFolder => Action::Run(Task::perform(
                rfd::AsyncFileDialog::new()
                    .set_title("Choose a folder for your model library...")
                    .pick_folder(),
                Message::PickedLibraryFolder,
            )),
            Message::PickedLibraryFolder(directory) => {
                let Some(directory) = directory else {
                    return Action::None;
                };

                Action::ChangeLibraryFolder(directory.path().to_path_buf())
            }
        }
    }

    pub fn view<'a>(&'a self, library: &model::Library, theme: &'a Theme) -> Element<'a, Message> {
        let section = match self.section {
            Section::Storage => self.storage(library),
            Section::Theme => self.theme(theme),
            Section::Mcp => self.mcp(),
        };

        center_y(scrollable(
            center_x(container(section).max_width(600)).padding(20),
        ))
        .into()
    }

    pub fn storage(&self, library: &model::Library) -> Element<'_, Message> {
        row![
            column![
                text("Model Library")
                    .font(Font {
                        weight: font::Weight::Semibold,
                        ..Font::MONOSPACE
                    })
                    .size(20),
                text("Models will be downloaded and stored in this directory.").width(Fill)
            ]
            .spacing(10),
            row![
                container(value(library.directory().path().display()).font(Font::MONOSPACE))
                    .width(300)
                    .padding(10)
                    .style(container::bordered_box),
                button(icon::folder_open()).on_press(Message::PickLibraryFolder),
            ]
            .align_y(Center)
            .spacing(10)
        ]
        .align_y(Center)
        .spacing(20)
        .into()
    }

    pub fn theme<'a>(&'a self, current: &'a Theme) -> Element<'a, Message> {
        let swatch = |color| {
            container(space::horizontal())
                .width(10)
                .height(10)
                .style(move |_theme: &Theme| {
                    container::Style::default()
                        .background(color)
                        .border(border::rounded(5))
                })
                .into()
        };

        let themes = self.themes.iter().map(|theme| {
            let palette = theme.palette();

            let item = button(
                ellipsized_text(theme.to_string())
                    .font(Font::MONOSPACE)
                    .width(Fill)
                    .wrapping(text::Wrapping::None)
                    .center(),
            )
            .on_press_with(|| Message::ChangeTheme(theme.clone()))
            .padding([10, 0])
            .style(move |_theme, status| {
                let mut style = button::background(theme, status);

                style.border = style.border.rounded(5);

                if current == theme {
                    button::Style {
                        border: style.border.color(current.palette().primary).width(3),
                        ..style
                    }
                } else {
                    style
                }
            });

            let swatches = {
                let colors = [palette.primary, palette.success, palette.danger];

                right_center(row(colors.into_iter().map(swatch)).spacing(5))
                    .padding(padding::right(10))
            };

            if current == theme {
                float(stack![item, swatches]).scale(1.1).into()
            } else {
                hover(item, swatches)
            }
        });

        container(grid(themes).spacing(10).fluid(300).height(Shrink)).into()
    }

    pub fn mcp(&self) -> Element<'_, Message> {
        button(
            column![
                mcp().width(100).height(100),
                typewriter("Coming Soon™")
                    .font(Font::MONOSPACE)
                    .size(30)
                    .very_slow(),
            ]
            .spacing(20)
            .align_x(Center),
        )
        .on_press(Message::OpenTechne)
        .style(button::text)
        .into()
    }

    pub fn sidebar(&self) -> Element<'_, Message> {
        let header = sidebar::header("Settings", None);

        let sections = [Section::Storage, Section::Theme, Section::Mcp]
            .into_iter()
            .map(|section| {
                sidebar::item(
                    row![section.icon(), text(section.title())]
                        .align_y(Center)
                        .spacing(10),
                    self.section == section,
                    move || Message::Open(section),
                )
            });

        column![header, scrollable(column(sections)).spacing(10)]
            .spacing(10)
            .into()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Section {
    Storage,
    Theme,
    Mcp,
}

impl Section {
    pub fn title(self) -> &'static str {
        match self {
            Self::Storage => "Storage",
            Self::Theme => "Theme",
            Self::Mcp => "MCP",
        }
    }

    pub fn icon(self) -> Element<'static, Message> {
        match self {
            Self::Storage => icon::folder().line_height(1.0).into(),
            Self::Theme => icon::palette().line_height(1.0).into(),
            Self::Mcp => mcp()
                .width(16)
                .height(16)
                .style(|theme: &Theme, _status| svg::Style {
                    color: Some(theme.palette().text),
                })
                .into(),
        }
    }
}

fn mcp() -> Svg<'static> {
    static ICON: LazyLock<svg::Handle> =
        LazyLock::new(|| svg::Handle::from_memory(include_bytes!("../../assets/mcp.svg")));

    svg(ICON.clone()).style(|theme: &Theme, _status| svg::Style {
        color: Some(theme.palette().text),
    })
}
