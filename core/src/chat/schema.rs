use crate::assistant;
use crate::chat::{Id, Item};
use crate::model;
use crate::plan;
use crate::Url;

use futures::never::Never;
use serde::de::{self, Deserializer, Error, MapAccess, Visitor};
use serde::{Deserialize, Serialize};

use std::fmt;
use std::marker::PhantomData;
use std::str::FromStr;
use std::time::Duration;

#[derive(Debug, Serialize, Deserialize)]
pub struct Schema {
    pub id: Id,
    pub file: model::File,
    pub title: Option<String>,
    pub history: Vec<Message>,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum Message {
    User(String),
    #[serde(rename = "Assistant", deserialize_with = "string_or_struct")]
    Reply(Reply),
    Plan(Plan),
}

impl Message {
    pub fn from_data(item: Item) -> Self {
        match item {
            Item::User(content) => Message::User(content),
            Item::Reply(reply) => Message::Reply(Reply {
                reasoning: reply
                    .reasoning
                    .as_ref()
                    .map(|reasoning| reasoning.content.clone())
                    .unwrap_or_default(),
                reasoning_time: reply
                    .reasoning
                    .map(|reasoning| reasoning.duration)
                    .unwrap_or_default(),
                content: reply.content,
            }),
            Item::Plan(plan) => Message::Plan(Plan::from_data(plan)),
        }
    }

    pub fn to_data(self) -> Item {
        match self {
            Message::User(content) => Item::User(content),
            Message::Reply(reply) => Item::Reply(reply.to_data()),
            Message::Plan(plan) => Item::Plan(plan.to_data()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Reply {
    reasoning: String,
    #[serde(default)]
    reasoning_time: Duration,
    content: String,
}

impl Reply {
    fn from_data(reply: assistant::Reply) -> Self {
        Reply {
            reasoning: reply
                .reasoning
                .as_ref()
                .map(|reasoning| reasoning.content.clone())
                .unwrap_or_default(),
            reasoning_time: reply
                .reasoning
                .map(|reasoning| reasoning.duration)
                .unwrap_or_default(),
            content: reply.content,
        }
    }

    fn to_data(self) -> assistant::Reply {
        assistant::Reply {
            reasoning: if self.reasoning.is_empty() {
                None
            } else {
                Some(assistant::Reasoning {
                    content: self.reasoning,
                    duration: self.reasoning_time,
                })
            },
            content: self.content,
        }
    }
}

impl FromStr for Reply {
    type Err = Never;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self {
            reasoning: String::new(),
            reasoning_time: Duration::default(),
            content: s.to_owned(),
        })
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct Reasoning {
    content: String,
    duration: Duration,
}

impl Reasoning {
    fn from_data(reasoning: assistant::Reasoning) -> Self {
        Self {
            content: reasoning.content,
            duration: reasoning.duration,
        }
    }

    fn to_data(self) -> assistant::Reasoning {
        assistant::Reasoning {
            content: self.content,
            duration: self.duration,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Plan {
    #[serde(default)]
    reasoning: Option<Reasoning>,
    #[serde(default)]
    steps: Vec<Step>,
    #[serde(default)]
    outcomes: Vec<Outcome>,
}

impl Plan {
    fn from_data(plan: crate::Plan) -> Self {
        Self {
            reasoning: plan.reasoning.map(Reasoning::from_data),
            steps: plan.steps.into_iter().map(Step::from_data).collect(),
            outcomes: plan.outcomes.into_iter().map(Outcome::from_data).collect(),
        }
    }

    fn to_data(self) -> crate::Plan {
        crate::Plan {
            reasoning: self.reasoning.map(Reasoning::to_data),
            steps: self.steps.into_iter().map(Step::to_data).collect(),
            outcomes: self.outcomes.into_iter().map(Outcome::to_data).collect(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Step {
    pub evidence: String,
    pub description: String,
    pub function: String,
    pub inputs: Vec<String>,
}

impl Step {
    fn from_data(step: plan::Step) -> Self {
        Self {
            evidence: step.evidence,
            description: step.description,
            function: step.function,
            inputs: step.inputs,
        }
    }

    fn to_data(self) -> plan::Step {
        plan::Step {
            evidence: self.evidence,
            description: self.description,
            function: self.function,
            inputs: self.inputs,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Outcome {
    Search(Status<Vec<Url>>),
    ScrapeText(Status<Vec<String>>),
    Answer(Status<Reply>),
}

impl Outcome {
    fn from_data(outcome: plan::Outcome) -> Self {
        match outcome {
            plan::Outcome::Search(status) => Self::Search(Status::from_data(status)),
            plan::Outcome::ScrapeText(status) => Self::ScrapeText(Status::from_data(status)),
            plan::Outcome::Answer(status) => {
                Self::Answer(Status::from_data(status.map(Reply::from_data)))
            }
        }
    }

    fn to_data(self) -> plan::Outcome {
        match self {
            Self::Search(status) => plan::Outcome::Search(Status::to_data(status)),
            Self::ScrapeText(status) => plan::Outcome::ScrapeText(Status::to_data(status)),
            Self::Answer(status) => {
                plan::Outcome::Answer(Status::to_data(status.map(Reply::to_data)))
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Status<T> {
    Active(T),
    Done(T),
    Errored(String),
}

impl<T> Status<T> {
    fn from_data(status: plan::Status<T>) -> Self {
        match status {
            plan::Status::Active(value) => Status::Active(value),
            plan::Status::Done(value) => Status::Done(value),
            plan::Status::Errored(error) => Status::Errored(error),
        }
    }

    fn to_data(self) -> plan::Status<T> {
        match self {
            Status::Active(value) => plan::Status::Active(value),
            Status::Done(value) => plan::Status::Done(value),
            Status::Errored(error) => plan::Status::Errored(error),
        }
    }

    fn map<A>(self, f: impl FnOnce(T) -> A) -> Status<A> {
        match self {
            Status::Active(value) => Status::Active(f(value)),
            Status::Done(value) => Status::Done(f(value)),
            Status::Errored(error) => Status::Errored(error),
        }
    }
}

fn string_or_struct<'de, T, D>(deserializer: D) -> Result<T, D::Error>
where
    T: Deserialize<'de> + FromStr<Err = Never>,
    D: Deserializer<'de>,
{
    // This is a Visitor that forwards string types to T's `FromStr` impl and
    // forwards map types to T's `Deserialize` impl. The `PhantomData` is to
    // keep the compiler from complaining about T being an unused generic type
    // parameter. We need T in order to know the Value type for the Visitor
    // impl.
    struct StringOrStruct<T>(PhantomData<fn() -> T>);

    impl<'de, T> Visitor<'de> for StringOrStruct<T>
    where
        T: Deserialize<'de> + FromStr<Err = Never>,
    {
        type Value = T;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("string or map")
        }

        fn visit_str<E>(self, value: &str) -> Result<T, E>
        where
            E: Error,
        {
            Ok(FromStr::from_str(value).unwrap())
        }

        fn visit_map<M>(self, map: M) -> Result<T, M::Error>
        where
            M: MapAccess<'de>,
        {
            // `MapAccessDeserializer` is a wrapper that turns a `MapAccess`
            // into a `Deserializer`, allowing it to be used as the input to T's
            // `Deserialize` implementation. T then deserializes itself using
            // the entries from the map visitor.
            Deserialize::deserialize(de::value::MapAccessDeserializer::new(map))
        }
    }

    deserializer.deserialize_any(StringOrStruct(PhantomData))
}
