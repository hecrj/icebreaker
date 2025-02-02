// Generated automatically by iced_fontello at build time.
// Do not edit manually. Source: ../fonts/icebreaker-icons.toml
// d0532da8de7e85e9222f7c911baa104de985f1b7f84f01e82bf71feb9968acce
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

pub fn refresh<'a>() -> Text<'a> {
    icon("\u{E760}")
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
