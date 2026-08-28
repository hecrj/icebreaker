pub mod sidebar;

pub mod tip {
    pub use super::tooltip::Position;
}

use crate::icon;

use iced::widget::{Text, button, container, iced, row, space, text, tooltip};
use iced::{Center, Element, Font};

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
        space::horizontal(),
        text("Powered by"),
        hugging_face,
        text("and"),
        llama_cpp,
    ]
    .spacing(7)
    .align_y(Center)
    .into()
}
