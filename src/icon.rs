// Generated automatically by iced_fontello at build time.
// Do not edit manually.
// f784a95f14b86b034e2cc96471e57ab257228d9ae0e188b24a0c3462f53121fd
use iced::widget::{text, Text};
use iced::Font;

pub const FONT: &[u8] = include_bytes!("../fonts/icebreaker-icons.ttf");

pub fn chat<'a>() -> Text<'a> {
    icon("\u{E720}")
}

pub fn clipboard<'a>() -> Text<'a> {
    icon("\u{1F4CB}")
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

pub fn trash<'a>() -> Text<'a> {
    icon("\u{F1F8}")
}

pub fn user<'a>() -> Text<'a> {
    icon("\u{1F464}")
}

fn icon<'a>(codepoint: &'a str) -> Text<'a> {
    text(codepoint).font(Font::with_name("icebreaker-icons"))
}
