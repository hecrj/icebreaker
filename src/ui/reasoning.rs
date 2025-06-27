use crate::core::assistant;
use crate::icon;

use iced::time::Duration;
use iced::widget::{button, column, row, text, vertical_rule};
use iced::{Element, Font, Shrink};

#[derive(Debug, Clone)]
pub struct Reasoning {
    pub thoughts: Vec<String>,
    pub duration: Duration,
    pub show: bool,
}

impl Reasoning {
    pub fn from_data(reasoning: assistant::Reasoning) -> Self {
        Self {
            thoughts: reasoning.content.split("\n\n").map(str::to_owned).collect(),
            duration: reasoning.duration,
            show: false,
        }
    }

    pub fn to_data(&self) -> assistant::Reasoning {
        assistant::Reasoning {
            content: self.thoughts.join("\n\n"),
            duration: self.duration,
        }
    }

    pub fn quote<Message>(&self, on_toggle: impl Fn(bool) -> Message) -> Element<'_, Message>
    where
        Message: Clone + 'static,
    {
        let toggle = button(
            row![
                text!(
                    "Reasoned for {duration} second{plural}",
                    duration = self.duration.as_secs(),
                    plural = if self.duration.as_secs() != 1 {
                        "s"
                    } else {
                        ""
                    }
                )
                .font(Font::MONOSPACE)
                .size(12),
                if self.show {
                    icon::arrow_down()
                } else {
                    icon::arrow_up()
                }
                .size(12),
            ]
            .spacing(10),
        )
        .on_press(on_toggle(!self.show))
        .style(button::secondary);

        let reasoning: Element<'_, _> = if self.show {
            let thoughts = column(self.thoughts.iter().map(|thought| {
                text(thought)
                    .size(12)
                    .shaping(text::Shaping::Advanced)
                    .into()
            }))
            .spacing(12);

            column![
                toggle,
                row![vertical_rule(1), thoughts].spacing(10).height(Shrink)
            ]
            .spacing(10)
            .into()
        } else {
            toggle.into()
        };

        reasoning
    }
}
