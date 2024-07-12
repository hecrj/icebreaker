use iced::widget::{text, Text};
use iced::Font;

pub const FONT_BYTES: &'static [u8] = include_bytes!("../fonts/icebreaker-icons.ttf");

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

pub fn clipboard<'a>() -> Text<'a> {
    with_codepoint('\u{E805}')
}

fn with_codepoint<'a>(codepoint: char) -> Text<'a> {
    const FONT: Font = Font::with_name("icebreaker-icons");

    text(codepoint).font(FONT)
}
