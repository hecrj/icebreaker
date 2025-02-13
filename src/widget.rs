use crate::icon;

use iced::widget::{button, container, text, tooltip, Text};
use iced::Element;

pub mod tip {
    pub use super::tooltip::Position;
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
