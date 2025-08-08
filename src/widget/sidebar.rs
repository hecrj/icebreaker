use iced::border;
use iced::widget::{button, row, text, Text};
use iced::{Center, Element, Fill, Font, Length, Shrink};

pub fn header<'a, Message: Clone + 'a>(
    title: impl text::IntoFragment<'a>,
    icon: Option<(Text<'a>, Message)>,
) -> Element<'a, Message> {
    let height = icon
        .is_none()
        .then_some(32)
        .map(Length::from)
        .unwrap_or(Shrink);

    row![
        text(title).width(Fill).font(Font::MONOSPACE),
        icon.map(|(icon, on_press)| {
            button(icon.line_height(1.0))
                .on_press(on_press)
                .padding([8, 10])
                .style(|theme, status| button::Style {
                    border: border::rounded(5),
                    ..button::subtle(theme, status)
                })
        })
    ]
    .height(height)
    .align_y(Center)
    .into()
}

pub fn item<'a, Message: Clone + 'a>(
    content: impl Into<Element<'a, Message>>,
    is_active: bool,
    on_press: impl Fn() -> Message + 'a,
) -> Element<'a, Message> {
    button(content)
        .on_press_with(on_press)
        .padding([8, 10])
        .width(Fill)
        .style(move |theme, status| {
            let base = button::Style {
                border: border::rounded(5),
                ..button::subtle(theme, status)
            };

            if is_active {
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
}
