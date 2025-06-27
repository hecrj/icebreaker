use crate::browser;
use crate::core::plan::{self, Event, Status, Step};
use crate::core::web;
use crate::core::{self, Url};
use crate::icon;
use crate::ui::markdown;
use crate::ui::{Reasoning, Reply};
use crate::widget::diffused_text;

use iced::border;
use iced::theme;
use iced::time::seconds;
use iced::widget::{
    button, center, center_x, column, container, horizontal_space, row, scrollable, text,
};
use iced::{Bottom, Center, Element, Fill, Font, Function, Task, Theme};

#[derive(Debug, Default)]
pub struct Plan {
    reasoning: Option<Reasoning>,
    steps: Vec<Step>,
    outcomes: Vec<Outcome>,
    active_step: Option<usize>,
}

#[derive(Debug, Clone)]
pub enum Message {
    ToggleAnswerReasoning(usize, bool),
    Markdown(markdown::Interaction),
    OpenLink(Url),
    ChangeStep(usize),
}

impl Plan {
    pub fn from_data(plan: core::Plan) -> Self {
        Self {
            reasoning: plan.reasoning.map(Reasoning::from_data),
            steps: plan.steps,
            outcomes: plan.outcomes.into_iter().map(Outcome::from_data).collect(),
            active_step: None,
        }
    }

    pub fn to_data(&self) -> core::Plan {
        core::Plan {
            reasoning: self.reasoning.as_ref().map(Reasoning::to_data),
            steps: self.steps.clone(),
            outcomes: self.outcomes.iter().map(Outcome::to_data).collect(),
        }
    }

    pub fn apply(&mut self, event: Event) {
        match event {
            Event::Designing(reasoning) => {
                self.reasoning = Some(Reasoning::from_data(reasoning));
            }
            Event::Designed(plan) => {
                self.reasoning = plan.reasoning.map(Reasoning::from_data);
                self.steps = plan.steps;
            }
            Event::OutcomeAdded(outcome) => {
                self.outcomes.push(Outcome::from_data(outcome));
            }
            Event::OutcomeChanged(new_outcome) => {
                let Some(Outcome::Answer(Status::Active(mut reply))) = self.outcomes.pop() else {
                    self.outcomes.push(Outcome::from_data(new_outcome));
                    return;
                };

                let plan::Outcome::Answer(new_status) = new_outcome else {
                    return;
                };

                self.outcomes
                    .push(Outcome::Answer(new_status.map(move |new_reply| {
                        reply.update(new_reply);
                        reply
                    })));
            }
        }
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::ToggleAnswerReasoning(index, show) => {
                if let Some(Outcome::Answer(Status::Done(reply))) = self.outcomes.get_mut(index) {
                    reply.toggle_reasoning(show);
                }

                Task::none()
            }
            Message::Markdown(interaction) => interaction.perform(),
            Message::OpenLink(url) => {
                browser::open(&url);

                Task::none()
            }
            Message::ChangeStep(step) => {
                self.active_step = Some(step);

                Task::none()
            }
        }
    }

    pub fn view(&self, theme: &Theme) -> Element<'_, Message> {
        let steps: Element<'_, _> = if self.steps.is_empty() {
            diffused_text("Designing a plan...")
                .size(20)
                .font(Font::MONOSPACE)
                .width(Fill)
                .duration(seconds(1))
                .center()
                .into()
        } else {
            let active_step = self
                .active_step
                .unwrap_or(self.outcomes.len().saturating_sub(1));

            let steps = center_x(
                row(self
                    .steps
                    .iter()
                    .zip(
                        self.outcomes
                            .iter()
                            .map(Some)
                            .chain(std::iter::repeat(None)),
                    )
                    .enumerate()
                    .flat_map(|(i, (_step, outcome))| {
                        let status = outcome.map(Outcome::stage).unwrap_or(Stage::Pending);
                        let pair = status.color(theme);

                        let number = center(text!("{}", i + 1).size(12).font(Font::MONOSPACE))
                            .width(24)
                            .height(24)
                            .style(move |_theme| {
                                let base = container::Style {
                                    border: border::rounded(12).color(pair.color).width(1),
                                    ..container::Style::default()
                                };

                                if i == active_step {
                                    container::Style {
                                        background: Some(pair.color.into()),
                                        text_color: Some(pair.text),
                                        ..base
                                    }
                                } else {
                                    container::Style {
                                        text_color: Some(pair.color),
                                        ..base
                                    }
                                }
                            });

                        let number: Element<'_, _> = if i == active_step {
                            number.into()
                        } else {
                            button(number)
                                .on_press(Message::ChangeStep(i))
                                .padding(0)
                                .style(button::text)
                                .into()
                        };

                        if i < self.steps.len() - 1 {
                            vec![number, icon::arrow_right().color(pair.color).into()]
                        } else {
                            vec![number]
                        }
                    }))
                .spacing(30)
                .align_y(Center),
            );

            let current: Element<'_, _> = self
                .steps
                .iter()
                .zip(self.outcomes.iter())
                .enumerate()
                .nth(active_step)
                .map(|(i, (step, outcome))| {
                    let status = outcome.stage();

                    let text_style = match status {
                        Stage::Pending => text::default,
                        Stage::Active => text::primary,
                        Stage::Done => text::success,
                        Stage::Error => text::danger,
                    };

                    let title = diffused_text(step.description.trim_matches('.'))
                        .font(Font::MONOSPACE)
                        .width(Fill)
                        .align_x(Center)
                        .style(text_style);

                    column![title, outcome.view(i, theme)].spacing(20).into()
                })
                .unwrap_or_else(|| horizontal_space().into());

            column![steps, current].spacing(10).into()
        };

        steps
    }
}

#[derive(Debug)]
pub enum Outcome {
    Search(Status<Vec<Url>>),
    ScrapeText(Status<Vec<web::Summary>>),
    Answer(Status<Reply>),
}

impl Outcome {
    pub fn from_data(outcome: plan::Outcome) -> Self {
        match outcome {
            plan::Outcome::Search(status) => Self::Search(status),
            plan::Outcome::ScrapeText(status) => Self::ScrapeText(status),
            plan::Outcome::Answer(status) => Self::Answer(status.map(Reply::from_data)),
        }
    }

    pub fn to_data(&self) -> plan::Outcome {
        match self {
            Outcome::Search(status) => plan::Outcome::Search(status.clone()),
            Outcome::ScrapeText(status) => plan::Outcome::ScrapeText(status.clone()),
            Outcome::Answer(status) => plan::Outcome::Answer(status.as_ref().map(Reply::to_data)),
        }
    }

    pub fn view(&self, index: usize, theme: &Theme) -> Element<'_, Message> {
        match self {
            Outcome::Search(status) => show_status(status, |search| links(search)),
            Outcome::ScrapeText(status) => {
                show_status(status, |summaries| summary_grid(summaries, self.stage()))
            }
            Outcome::Answer(status) => show_status(status, |value| reply(value, index, theme)),
        }
    }

    fn stage(&self) -> Stage {
        let status = match self {
            Outcome::Search(status) => status.as_ref().map(|_| ()),
            Outcome::ScrapeText(status) => status.as_ref().map(|_| ()),
            Outcome::Answer(status) => status.as_ref().map(|_| ()),
        };

        match status {
            Status::Active(_) => Stage::Active,
            Status::Done(_) => Stage::Done,
            Status::Errored(_) => Stage::Error,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum Stage {
    Pending,
    Active,
    Error,
    Done,
}

impl Stage {
    fn color(self, theme: &Theme) -> theme::palette::Pair {
        let palette = theme.extended_palette();

        match self {
            Stage::Pending => palette.secondary.base,
            Stage::Active => palette.primary.base,
            Stage::Done => palette.success.base,
            Stage::Error => palette.danger.base,
        }
    }
}

fn show_status<'a, T>(
    status: &'a Status<T>,
    show: impl Fn(&'a T) -> Element<'a, Message>,
) -> Element<'a, Message> {
    status.result().map(show).unwrap_or_else(error)
}

fn error(error: &str) -> Element<'_, Message> {
    text(error).style(text::danger).font(Font::MONOSPACE).into()
}

fn links(links: &[Url]) -> Element<'_, Message> {
    if links.is_empty() {
        return horizontal_space().into();
    }

    column(links.iter().map(|link| {
        button(
            column![
                row![
                    text(
                        link.host_str()
                            .unwrap_or(link.as_str())
                            .trim_start_matches("www.")
                    )
                    .font(Font::MONOSPACE),
                    icon::link().size(14)
                ]
                .align_y(Bottom)
                .spacing(10),
                text(link.path()).size(10).font(Font::MONOSPACE),
            ]
            .spacing(5),
        )
        .on_press_with(|| Message::OpenLink(link.clone()))
        .padding(0)
        .style(button::text)
        .into()
    }))
    .spacing(20)
    .into()
}

fn summary_grid(summaries: &[web::Summary], stage: Stage) -> Element<'_, Message> {
    fn summary(summary: &web::Summary, stage: Stage) -> Element<'_, Message> {
        let title = {
            let domain = text(
                summary
                    .url
                    .host_str()
                    .unwrap_or(summary.url.as_str())
                    .trim_start_matches("www."),
            )
            .font(Font::MONOSPACE)
            .wrapping(text::Wrapping::None);

            container(
                button(
                    row![domain, icon::link().size(14)]
                        .spacing(10)
                        .align_y(Bottom),
                )
                .on_press_with(|| Message::OpenLink(summary.url.clone()))
                .padding(0)
                .style(button::text),
            )
            .width(Fill)
            .clip(true)
        };

        let content = {
            let lines = column(
                summary
                    .content
                    .lines()
                    .map(|line| text(line).size(12).font(Font::MONOSPACE).into()),
            )
            .width(Fill)
            .spacing(5);

            container(scrollable(lines).spacing(5).anchor_y(match stage {
                Stage::Done => scrollable::Anchor::Start,
                _ => scrollable::Anchor::End,
            }))
            .max_height(150)
            .padding(10)
            .style(container::dark)
        };

        column![title, content].spacing(5).into()
    }

    column(summaries.iter().map(|item| summary(item, stage)))
        .spacing(20)
        .into()
}

fn reply<'a>(reply: &'a Reply, index: usize, theme: &Theme) -> Element<'a, Message> {
    reply.view(
        theme,
        Message::ToggleAnswerReasoning.with(index),
        Message::Markdown,
    )
}
