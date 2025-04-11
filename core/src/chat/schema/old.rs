use crate::assistant;
use crate::chat::{Id, Item};
use crate::model;
use crate::plan;
use crate::web;
use crate::Url;

use futures::never::Never;
use serde::de::{self, Deserializer, Error, MapAccess, Visitor};
use serde::Deserialize;

use std::fmt;
use std::marker::PhantomData;
use std::str::FromStr;
use std::time::Duration;

#[derive(Debug, Deserialize)]
pub struct Schema {
    pub id: Id,
    pub file: model::File,
    pub title: Option<String>,
    pub history: Vec<Message>,
}

#[derive(Debug, Deserialize)]
pub enum Message {
    User(String),
    #[serde(rename = "Assistant", deserialize_with = "string_or_struct")]
    Reply(Reply),
    Plan(Plan),
}

impl Message {
    pub fn into_data(self) -> Item {
        match self {
            Message::User(content) => Item::User(content),
            Message::Reply(reply) => Item::Reply(reply.into_data()),
            Message::Plan(plan) => Item::Plan(plan.into_data()),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct Reply {
    reasoning: String,
    #[serde(default)]
    reasoning_time: Duration,
    content: String,
}

impl Reply {
    fn into_data(self) -> assistant::Reply {
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
            last_token: None,
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

#[derive(Debug, Deserialize)]
struct Reasoning {
    content: String,
    duration: Duration,
}

impl Reasoning {
    fn into_data(self) -> assistant::Reasoning {
        assistant::Reasoning {
            content: self.content,
            duration: self.duration,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct Plan {
    #[serde(default)]
    reasoning: Option<Reasoning>,
    #[serde(default)]
    steps: Vec<Step>,
    #[serde(default)]
    outcomes: Vec<Outcome>,
}

impl Plan {
    fn into_data(self) -> crate::Plan {
        crate::Plan {
            reasoning: self.reasoning.map(Reasoning::into_data),
            steps: self.steps.into_iter().map(Step::into_data).collect(),
            outcomes: self.outcomes.into_iter().map(Outcome::into_data).collect(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct Step {
    pub evidence: String,
    pub description: String,
    pub function: String,
    pub inputs: Vec<String>,
}

impl Step {
    fn into_data(self) -> plan::Step {
        plan::Step {
            evidence: self.evidence,
            description: self.description,
            function: self.function,
            inputs: self.inputs,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub enum Outcome {
    Search(Status<Vec<Url>>),
    ScrapeText(Status<WebSummaries>),
    Answer(Status<Reply>),
}

impl Outcome {
    fn into_data(self) -> plan::Outcome {
        match self {
            Self::Search(status) => plan::Outcome::Search(Status::into_data(status)),
            Self::ScrapeText(status) => plan::Outcome::ScrapeText(Status::into_data(status.map(
                |summaries| match summaries {
                    WebSummaries::Known(summaries) => {
                        summaries.into_iter().map(WebSummary::into_data).collect()
                    }
                    WebSummaries::Uknown(lines) => vec![web::Summary {
                        url: Url::parse("https://unknown.com/").expect("Parse URL"),
                        content: lines.join("\n"),
                    }],
                },
            ))),
            Self::Answer(status) => {
                plan::Outcome::Answer(Status::into_data(status.map(Reply::into_data)))
            }
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub enum Status<T> {
    Active(T),
    Done(T),
    Errored(String),
}

impl<T> Status<T> {
    fn into_data(self) -> plan::Status<T> {
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

#[derive(Debug, Clone, Deserialize)]
pub enum WebSummaries {
    Known(Vec<WebSummary>),
    #[serde(untagged)]
    Uknown(Vec<String>),
}

#[derive(Debug, Clone, Deserialize)]
pub struct WebSummary {
    url: Url,
    content: String,
}

impl WebSummary {
    fn into_data(self) -> web::Summary {
        web::Summary {
            url: self.url,
            content: self.content,
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
