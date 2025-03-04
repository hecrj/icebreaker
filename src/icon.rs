// Generated automatically by iced_fontello at build time.
// Do not edit manually. Source: ../fonts/icebreaker-icons.toml
// 8e94c2691fd8f7ef54e0c9d68ddc9c0c32db5b08851d08c814690922b7fbb3aa
use iced::widget::{text, Text};
use iced::Font;

pub const FONT: &[u8] = include_bytes!("../fonts/icebreaker-icons.ttf");

pub fn arrow_down<'a>() -> Text<'a> {
    icon("\u{E75C}")
}

pub fn arrow_right<'a>() -> Text<'a> {
    icon("\u{E75E}")
}

pub fn arrow_up<'a>() -> Text<'a> {
    icon("\u{E75F}")
}

pub fn clipboard<'a>() -> Text<'a> {
    icon("\u{F0C5}")
}

pub fn clock<'a>() -> Text<'a> {
    icon("\u{1F554}")
}

pub fn collapse<'a>() -> Text<'a> {
    icon("\u{F191}")
}

pub fn download<'a>() -> Text<'a> {
    icon("\u{1F4E5}")
}

pub fn expand<'a>() -> Text<'a> {
    icon("\u{F152}")
}

pub fn globe<'a>() -> Text<'a> {
    icon("\u{E02F}")
}

pub fn heart<'a>() -> Text<'a> {
    icon("\u{2665}")
}

pub fn link<'a>() -> Text<'a> {
    icon("\u{F08E}")
}

pub fn refresh<'a>() -> Text<'a> {
    icon("\u{E01E}")
}

pub fn trash<'a>() -> Text<'a> {
    icon("\u{F1F8}")
}

pub fn user<'a>() -> Text<'a> {
    icon("\u{1F464}")
}

fn icon(codepoint: &str) -> Text<'_> {
    text(codepoint).font(Font::with_name("icebreaker-icons"))
}
