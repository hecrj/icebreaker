use iced::widget::{text, Text};
use iced::Font;

pub fn download<'a>() -> Text<'a> {
    with_codepoint('\u{E800}')
}

pub fn heart<'a>() -> Text<'a> {
    with_codepoint('\u{E801}')
}

pub fn clock<'a>() -> Text<'a> {
    with_codepoint('\u{E802}')
}

pub fn user<'a>() -> Text<'a> {
    with_codepoint('\u{E803}')
}

pub fn chat<'a>() -> Text<'a> {
    with_codepoint('\u{E804}')
}

fn with_codepoint<'a>(codepoint: char) -> Text<'a> {
    const FONT: Font = Font::with_name("chat-icons");

    text(codepoint).font(FONT)
}
