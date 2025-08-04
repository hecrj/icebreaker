pub use iced_palace::widget::diffused_text;

pub mod tip {
    pub use super::tooltip::Position;
}

use crate::icon;

use iced::border;
use iced::widget::{button, container, horizontal_space, iced, row, text, tooltip, Button, Text};
use iced::{Center, Element, Fill, Font, Theme};

pub fn tip<'a, Message: 'a>(
    target: impl Into<Element<'a, Message>>,
    tip: &'a str,
    position: tip::Position,
) -> Element<'a, Message> {
    tooltip(
        target,
        container(text(tip).size(14))
            .padding(5)
            .style(container::dark),
        position,
    )
    .into()
}

pub fn toggle<'a, Message: 'a>(
    icon: Text<'a>,
    label: &'a str,
    is_toggled: bool,
) -> Button<'a, Message> {
    button(
        row![icon.size(12), text(label).size(12)]
            .spacing(8)
            .height(Fill)
            .align_y(Center),
    )
    .height(30)
    .style(move |theme: &Theme, status| {
        if is_toggled {
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
                    background: Some(palette.background.base.text.scale_alpha(0.2).into()),
                    ..base
                },
                button::Status::Disabled => button::Style::default(),
            }
        }
    })
}

pub fn copy<'a, Message>(on_press: impl Fn() -> Message + 'a) -> Element<'a, Message>
where
    Message: Clone + 'a,
{
    action(icon::clipboard(), "Copy", on_press)
}

pub fn regenerate<'a, Message>(on_press: impl Fn() -> Message + 'a) -> Element<'a, Message>
where
    Message: Clone + 'a,
{
    action(icon::refresh(), "Regenerate", on_press)
}

pub fn action<'a, Message>(
    icon: Text<'a>,
    label: &'a str,
    message: impl Fn() -> Message + 'a,
) -> Element<'a, Message>
where
    Message: Clone + 'a,
{
    tip(
        button(icon)
            .on_press_with(message)
            .padding([2, 7])
            .style(button::text),
        label,
        tip::Position::Bottom,
    )
}

#[derive(Debug, Clone, Copy)]
pub enum Link {
    Rust,
    Iced,
    HuggingFace,
    LlamaCpp,
}

impl Link {
    #[allow(dead_code)]
    pub fn url(self) -> &'static str {
        match self {
            Self::Rust => "https://rust-lang.org",
            Self::Iced => "https://iced.rs",
            Self::HuggingFace => "https://huggingface.co",
            Self::LlamaCpp => "https://github.com/ggerganov/llama.cpp",
        }
    }
}

#[allow(dead_code)]
pub fn about<'a>() -> Element<'a, Link> {
    let text = |content| text(content).font(Font::MONOSPACE).size(12);

    let link = |button: button::Button<'static, Link>, link| {
        button.on_press(link).padding(0).style(button::text)
    };

    let rust = link(
        button(text("🦀 Rust").shaping(text::Shaping::Advanced)),
        Link::Rust,
    );

    let iced = link(button(iced(12)), Link::Iced);

    let hugging_face = link(
        button(text("🤗 Hugging Face").shaping(text::Shaping::Advanced)),
        Link::HuggingFace,
    );

    let llama_cpp = link(
        button(text("🦙 llama.cpp").shaping(text::Shaping::Advanced)),
        Link::LlamaCpp,
    );

    row![
        text("Made with"),
        rust,
        text("and"),
        iced,
        horizontal_space(),
        text("Powered by"),
        hugging_face,
        text("and"),
        llama_cpp,
    ]
    .spacing(7)
    .align_y(Center)
    .into()
}

pub fn sidebar_section<'a, Message: Clone + 'a>(
    title: impl text::IntoFragment<'a>,
    icon: Text<'a>,
    on_icon_press: Message,
) -> Element<'a, Message> {
    row![
        text(title).width(Fill).font(Font::MONOSPACE),
        button(icon.line_height(1.0))
            .on_press(on_icon_press)
            .padding([8, 10])
            .style(|theme, status| {
                button::Style {
                    border: border::rounded(5),
                    ..button::subtle(theme, status)
                }
            })
    ]
    .align_y(Center)
    .into()
}
