// Generated automatically by iced_fontello at build time.
// Do not edit manually. Source: ../fonts/icebreaker-icons.toml
// 7eae0381e077a3f30ca1d09a78769f48e794f984dade7f6d777793faa4fde3ca
use iced::widget::{text, Text};
use iced::Font;

pub const FONT: &[u8] = include_bytes!("../fonts/icebreaker-icons.ttf");

pub fn arrow_down<'a>() -> Text<'a> {
    icon("\u{E75C}")
}

pub fn arrow_up<'a>() -> Text<'a> {
    icon("\u{E75F}")
}

pub fn chat<'a>() -> Text<'a> {
    icon("\u{E720}")
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

pub fn heart<'a>() -> Text<'a> {
    icon("\u{2665}")
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

fn icon<'a>(codepoint: &'a str) -> Text<'a> {
    text(codepoint).font(Font::with_name("icebreaker-icons"))
}
