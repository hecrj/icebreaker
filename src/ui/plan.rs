use crate::core::plan::{self, Event, Status, Step};
use crate::core::web;
use crate::core::{self, Url};
use crate::ui::markdown;
use crate::ui::{Reasoning, Reply};

use iced::border;
use iced::padding;
use iced::theme;
use iced::widget::{center, column, container, row, scrollable, text};
use iced::{Center, Element, Fill, Font, Function, Task, Theme};

#[derive(Debug, Default)]
pub struct Plan {
    reasoning: Option<Reasoning>,
    steps: Vec<Step>,
    outcomes: Vec<Outcome>,
}

#[derive(Debug, Clone)]
pub enum Message {
    ToggleReasoning(bool),
    ToggleAnswerReasoning(usize, bool),
    Markdown(markdown::Interaction),
}

impl Plan {
    pub fn from_data(plan: core::Plan) -> Self {
        Self {
            reasoning: plan.reasoning.map(Reasoning::from_data),
            steps: plan.steps,
            outcomes: plan.outcomes.into_iter().map(Outcome::from_data).collect(),
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
            Message::ToggleReasoning(show) => {
                if let Some(reasoning) = &mut self.reasoning {
                    reasoning.show = show;
                }

                Task::none()
            }
            Message::ToggleAnswerReasoning(index, show) => {
                if let Some(Outcome::Answer(Status::Done(reply))) = self.outcomes.get_mut(index) {
                    reply.toggle_reasoning(show);
                }

                Task::none()
            }
            Message::Markdown(interaction) => interaction.perform(),
        }
    }

    pub fn view(&self, theme: &Theme) -> Element<Message> {
        let steps: Element<_> = if self.steps.is_empty() {
            text("Designing a plan...")
                .font(Font::MONOSPACE)
                .width(Fill)
                .center()
                .into()
        } else {
            column(
                self.steps
                    .iter()
                    .zip(
                        self.outcomes
                            .iter()
                            .map(Some)
                            .chain(std::iter::repeat(None)),
                    )
                    .enumerate()
                    .map(|(n, (step, outcome))| {
                        let status = outcome.map(Outcome::stage).unwrap_or(Stage::Pending);

                        let text_style = match status {
                            Stage::Pending => text::default,
                            Stage::Active => text::primary,
                            Stage::Done => text::success,
                            Stage::Error => text::danger,
                        };

                        let number = center(
                            text!("{}", n + 1)
                                .size(12)
                                .font(Font::MONOSPACE)
                                .style(text_style),
                        )
                        .width(24)
                        .height(24)
                        .style(move |theme| {
                            let pair = status.color(theme);

                            container::Style::default()
                                .border(border::rounded(8).color(pair.color).width(1))
                        });

                        let title = row![
                            number,
                            text(&step.description)
                                .font(Font::MONOSPACE)
                                .style(text_style)
                        ]
                        .spacing(20)
                        .align_y(Center);

                        let step: Element<_> = if let Some(outcome) = outcome {
                            column![
                                title,
                                container(outcome.view(n, theme)).padding(padding::left(44))
                            ]
                            .spacing(10)
                            .into()
                        } else {
                            title.into()
                        };

                        step
                    }),
            )
            .spacing(30)
            .into()
        };

        if let Some(reasoning) = &self.reasoning {
            column![reasoning.quote(Message::ToggleReasoning), steps]
                .spacing(30)
                .into()
        } else {
            steps.into()
        }
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

    pub fn view(&self, index: usize, theme: &Theme) -> Element<Message> {
        match self {
            Outcome::Search(status) => show_status(status, links),
            Outcome::ScrapeText(status) => show_status(status, summary_grid),
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

fn error(error: &str) -> Element<Message> {
    text(error).style(text::danger).font(Font::MONOSPACE).into()
}

fn links(links: &Vec<Url>) -> Element<Message> {
    container(
        container(
            column(
                links
                    .iter()
                    .map(|link| text(link.as_str()).size(12).font(Font::MONOSPACE).into()),
            )
            .spacing(5),
        )
        .width(Fill)
        .padding(10)
        .style(container::dark),
    )
    .into()
}

fn summary_grid(summaries: &Vec<web::Summary>) -> Element<Message> {
    fn summary(summary: &web::Summary) -> Element<Message> {
        let title = text(summary.url.as_str())
            .size(14)
            .font(Font::MONOSPACE)
            .wrapping(text::Wrapping::None);

        let content = scrollable(
            column(
                summary
                    .content
                    .lines()
                    .map(|line| text(line).size(12).font(Font::MONOSPACE).into()),
            )
            .width(Fill)
            .spacing(5),
        )
        .spacing(5)
        .anchor_bottom();

        container(column![title, content].spacing(10))
            .clip(true)
            .width(Fill)
            .padding(10)
            .height(150)
            .style(container::dark)
            .into()
    }

    column(summaries.iter().map(summary)).spacing(10).into()
}

fn reply<'a>(reply: &'a Reply, index: usize, theme: &Theme) -> Element<'a, Message> {
    reply.view(
        theme,
        Message::ToggleAnswerReasoning.with(index),
        Message::Markdown,
    )
}
