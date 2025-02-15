mod diffused_text;

pub use diffused_text::DiffusedText;

pub mod tip {
    pub use super::tooltip::Position;
}

use crate::icon;

use iced::advanced;
use iced::border;
use iced::widget::{button, container, row, text, tooltip, Button, Text};
use iced::{Center, Element, Fill, Theme};

pub fn diffused_text<'a, Theme, Renderer>(
    fragment: impl text::IntoFragment<'a>,
) -> DiffusedText<'a, Theme, Renderer>
where
    Theme: text::Catalog,
    Renderer: advanced::text::Renderer,
{
    DiffusedText::new(fragment)
}

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
