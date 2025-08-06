// Generated automatically by iced_fontello at build time.
// Do not edit manually. Source: ../fonts/icebreaker-icons.toml
// e7bfc39bcd6eb36545284b94b7a695ad64e95827aa1cb1752630f30555e8bbe1
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

pub fn chat<'a>() -> Text<'a> {
    icon("\u{E0AC}")
}

pub fn check<'a>() -> Text<'a> {
    icon("\u{2713}")
}

pub fn clipboard<'a>() -> Text<'a> {
    icon("\u{F0C5}")
}

pub fn clock<'a>() -> Text<'a> {
    icon("\u{1F554}")
}

pub fn cog<'a>() -> Text<'a> {
    icon("\u{2699}")
}

pub fn cubes<'a>() -> Text<'a> {
    icon("\u{F1B3}")
}

pub fn download<'a>() -> Text<'a> {
    icon("\u{1F4E5}")
}

pub fn folder_open<'a>() -> Text<'a> {
    icon("\u{1F4C2}")
}

pub fn globe<'a>() -> Text<'a> {
    icon("\u{E02F}")
}

pub fn left<'a>() -> Text<'a> {
    icon("\u{E00E}")
}

pub fn link<'a>() -> Text<'a> {
    icon("\u{F08E}")
}

pub fn plus<'a>() -> Text<'a> {
    icon("\u{2B}")
}

pub fn refresh<'a>() -> Text<'a> {
    icon("\u{E01E}")
}

pub fn search<'a>() -> Text<'a> {
    icon("\u{1F50D}")
}

pub fn server<'a>() -> Text<'a> {
    icon("\u{F233}")
}

pub fn sliders<'a>() -> Text<'a> {
    icon("\u{F1DE}")
}

pub fn star<'a>() -> Text<'a> {
    icon("\u{2605}")
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
