use crate::browser;
use crate::widget::copy;

use iced::clipboard;
use iced::widget::{container, hover, markdown, right};
use iced::{Element, Never, Task, Theme};

#[derive(Debug, Default)]
pub struct Markdown {
    content: markdown::Content,
}

impl Markdown {
    pub fn parse(markdown: &str) -> Self {
        let content = markdown::Content::parse(markdown);

        Self { content }
    }

    pub fn push_str(&mut self, markdown: &str) {
        self.content.push_str(markdown);
    }

    pub fn view(&self, theme: &Theme) -> Element<'_, Interaction> {
        markdown::view_with(self.content.items(), theme, &Viewer)
    }
}

struct Viewer;

#[derive(Debug, Clone)]
pub enum Interaction {
    Open(markdown::Uri),
    Copy(String),
}

impl Interaction {
    pub fn perform(self) -> Task<Never> {
        match self {
            Interaction::Open(url) => {
                browser::open(url);

                Task::none()
            }
            Interaction::Copy(text) => clipboard::write(text).discard(),
        }
    }
}

impl<'a> markdown::Viewer<'a, Interaction> for Viewer {
    fn on_link_click(url: markdown::Uri) -> Interaction {
        Interaction::Open(url)
    }

    fn code_block(
        &self,
        settings: markdown::Settings,
        _language: Option<&'a str>,
        code: &'a str,
        lines: &'a [markdown::Text],
    ) -> Element<'a, Interaction> {
        let code_block = markdown::code_block(settings, lines, Interaction::Open);
        let copy = copy(|| Interaction::Copy(code.to_owned()));

        hover(
            code_block,
            right(container(copy).style(container::dark)).padding(settings.code_size / 2),
        )
    }
}
