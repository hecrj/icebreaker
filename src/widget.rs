use iced::widget::{container, text, tooltip};
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
