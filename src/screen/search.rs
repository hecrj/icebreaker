use crate::data::{Error, Model};
use crate::icon;

use iced::time::Duration;
use iced::widget::{
    self, button, center, column, container, horizontal_space, hover, iced, row, scrollable, text,
    text_input, value,
};
use iced::window;
use iced::{Center, Element, Fill, Font, Right, Size, Subscription, Task, Theme};

pub struct Search {
    models: Vec<Model>,
    search: String,
    search_temperature: usize,
    is_searching: bool,
    error: Option<Error>,
    window_size: Size,
}

#[derive(Debug, Clone)]
pub enum Message {
    ModelsListed(Result<Vec<Model>, Error>),
    SearchChanged(String),
    SearchCooled,
    RunModel(Model),
    LinkPressed(Link),
    WindowResized(Size),
}

#[derive(Debug, Clone)]
pub enum Link {
    Rust,
    Iced,
    HuggingFace,
    LlamaCpp,
}

pub enum Action {
    None,
    Run(Task<Message>),
    Boot(Model),
}

impl Search {
    pub fn new() -> (Self, Task<Message>) {
        (
            Self {
                models: Vec::new(),
                search: String::new(),
                search_temperature: 0,
                is_searching: true,
                error: None,
                window_size: Size::ZERO,
            },
            Task::batch([
                Task::perform(Model::list(), Message::ModelsListed),
                widget::focus_next(),
                window::get_latest()
                    .and_then(window::get_size)
                    .map(Message::WindowResized),
            ]),
        )
    }

    pub fn title(&self) -> String {
        "Icebreaker".to_owned()
    }

    pub fn update(&mut self, message: Message) -> Action {
        match message {
            Message::ModelsListed(Ok(models)) => {
                self.models = models;
                self.is_searching = false;

                Action::None
            }
            Message::ModelsListed(Err(error)) => {
                self.error = Some(dbg!(error));

                Action::None
            }
            Message::SearchChanged(search) => {
                self.search = search;
                self.search_temperature += 1;

                Action::Run(Task::perform(
                    tokio::time::sleep(Duration::from_secs(1)),
                    |_| Message::SearchCooled,
                ))
            }
            Message::SearchCooled => {
                self.search_temperature = self.search_temperature.saturating_sub(1);

                if self.search_temperature == 0 {
                    self.is_searching = true;

                    Action::Run(Task::perform(
                        Model::search(self.search.clone()),
                        Message::ModelsListed,
                    ))
                } else {
                    Action::None
                }
            }
            Message::RunModel(model) => Action::Boot(model),
            Message::LinkPressed(link) => {
                let _ = open::that_in_background(match link {
                    Link::Rust => "https://rust-lang.org",
                    Link::Iced => "https://iced.rs",
                    Link::HuggingFace => "https://huggingface.co",
                    Link::LlamaCpp => "https://github.com/ggerganov/llama.cpp",
                });

                Action::None
            }
            Message::WindowResized(size) => {
                self.window_size = size;

                Action::None
            }
        }
    }

    pub fn view(&self) -> Element<Message> {
        let search = text_input("Search language models...", &self.search)
            .size(20)
            .padding(10)
            .on_input(Message::SearchChanged);

        let models: Element<_> =
            {
                let search_terms: Vec<_> = self
                    .search
                    .trim()
                    .split(' ')
                    .map(str::to_lowercase)
                    .collect();

                let mut filtered_models = self
                    .models
                    .iter()
                    .filter(|model| {
                        self.search.is_empty()
                            || search_terms
                                .iter()
                                .all(|term| model.name().to_lowercase().contains(term))
                    })
                    .peekable();

                if filtered_models.peek().is_none() {
                    center(text(if self.is_searching || self.search_temperature > 0 {
                        "Searching..."
                    } else {
                        "No models found!"
                    }))
                    .into()
                } else {
                    use itertools::Itertools;
                    const MIN_CARD_WIDTH: f32 = 450.0;

                    let n_columns = (self.window_size.width / MIN_CARD_WIDTH).max(1.0) as usize;

                    let cards =
                        column(filtered_models.chunks(n_columns).into_iter().map(|chunk| {
                            row(chunk.into_iter().map(model_card)).spacing(10).into()
                        }))
                        .spacing(10);

                    scrollable(cards).height(Fill).spacing(10).into()
                }
            };

        let footer = {
            let text = |content| text(content).font(Font::MONOSPACE).size(12);

            let link = |button: button::Button<'static, Message>, link| {
                button
                    .on_press(Message::LinkPressed(link))
                    .padding(0)
                    .style(button::text)
            };

            let rust = link(
                button(text("🦀 Rust").shaping(text::Shaping::Advanced)),
                Link::Rust,
            );

            let iced = link(button(iced(12)), Link::Iced);

            let hugging_face = link(
                button(text("🤗 Hugging Face").shaping(text::Shaping::Advanced)),
                Link::HuggingFace,
            );

            let llama_cpp = link(
                button(text("🦙 llama.cpp").shaping(text::Shaping::Advanced)),
                Link::LlamaCpp,
            );

            row![
                text("Made with"),
                rust,
                text("and"),
                iced,
                horizontal_space(),
                text("Powered by"),
                hugging_face,
                text("and"),
                llama_cpp,
            ]
            .spacing(7)
            .align_y(Center)
        };

        container(column![search, models, footer].spacing(10))
            .padding(10)
            .into()
    }

    pub fn subscription(&self) -> Subscription<Message> {
        window::resize_events().map(|(_id, size)| Message::WindowResized(size))
    }
}

fn model_card(model: &Model) -> Element<Message> {
    use iced::widget::Text;

    fn stat<'a>(
        icon: Text<'a>,
        value: Text<'a>,
        style: fn(&Theme) -> text::Style,
    ) -> Element<'a, Message> {
        row![
            icon.size(10).style(style),
            value.size(12).font(Font::MONOSPACE).style(style)
        ]
        .align_y(Center)
        .spacing(5)
        .into()
    }

    let title = {
        const LIMIT: usize = 40;

        let name = model.name();

        if name.len() < LIMIT {
            text(name)
        } else {
            text!("{}...", &name[0..LIMIT])
        }
        .font(Font::MONOSPACE)
    };

    let separator = || text("•").size(12);

    let metadata = row![
        stat(icon::user(), text(model.author()), text::default),
        separator(),
        stat(icon::download(), value(model.downloads), text::primary),
        separator(),
        stat(icon::heart(), value(model.likes), text::danger),
        separator(),
        stat(
            icon::clock(),
            value(model.last_modified.format("%-e %B, %y")),
            text::default,
        ),
    ]
    .spacing(10);

    let chat = container(
        button(row![icon::chat(), "Run"].spacing(10)).on_press(Message::RunModel(model.clone())),
    )
    .width(Fill)
    .padding(10)
    .align_x(Right)
    .center_y(Fill);

    let card = container(column![title, metadata].spacing(10))
        .width(Fill)
        .padding(10)
        .style(container::rounded_box);

    hover(card, chat)
}
