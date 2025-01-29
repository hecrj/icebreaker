use crate::data::assistant;
use crate::data::chat::Id;

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
    pub file: assistant::File,
    pub title: Option<String>,
    pub history: Vec<Message>,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum Message {
    User(String),
    #[serde(deserialize_with = "string_or_struct")]
    Assistant(AssistantMessage),
}

impl From<assistant::Message> for Message {
    fn from(message: assistant::Message) -> Self {
        match message {
            assistant::Message::User(content) => Message::User(content),
            assistant::Message::Assistant { reasoning, content } => {
                Message::Assistant(AssistantMessage {
                    reasoning: reasoning
                        .as_ref()
                        .map(|reasoning| reasoning.content.clone())
                        .unwrap_or_default(),
                    reasoning_time: reasoning
                        .map(|reasoning| reasoning.duration)
                        .unwrap_or_default(),
                    content,
                })
            }
        }
    }
}

impl From<Message> for assistant::Message {
    fn from(message: Message) -> Self {
        match message {
            Message::User(content) => assistant::Message::User(content),
            Message::Assistant(message) => assistant::Message::Assistant {
                reasoning: if message.reasoning.is_empty() {
                    None
                } else {
                    Some(assistant::Reasoning {
                        content: message.reasoning,
                        duration: message.reasoning_time,
                    })
                },
                content: message.content,
            },
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AssistantMessage {
    reasoning: String,
    #[serde(default)]
    reasoning_time: Duration,
    content: String,
}

impl FromStr for AssistantMessage {
    type Err = Never;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self {
            reasoning: String::new(),
            reasoning_time: Duration::default(),
            content: s.to_owned(),
        })
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
