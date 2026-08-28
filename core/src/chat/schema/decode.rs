use crate::Chat;
use crate::assistant::{Reasoning, Reply};
use crate::chat;
use crate::model;

use decoder::decode::{duration, map, sequence, string};
use decoder::{Error, Result, Value};

pub fn chat(value: Value) -> Result<Chat> {
    let mut chat = map(value)?;

    Ok(Chat {
        id: chat.required("id", chat::Id::decode)?,
        file: chat.required("file", model::File::decode)?,
        title: chat.optional("title", string)?,
        history: chat.required("history", sequence(item))?,
    })
}

fn item(value: Value) -> Result<chat::Item> {
    let mut item = map(value)?;

    let type_ = item.required("type", string)?;

    let item = match type_.as_str() {
        "user" => chat::Item::User(item.required("message", string)?),
        "reply" => chat::Item::Reply(reply(item.into_value())?),
        _ => {
            return Err(Error::custom(format!("invalid chat item: {type_}")));
        }
    };

    Ok(item)
}

fn reply(value: Value) -> Result<Reply> {
    let mut reply = map(value)?;

    Ok(Reply {
        reasoning: reply.optional("reasoning", reasoning)?,
        content: reply.required("content", string)?,
        last_token: None,
    })
}

fn reasoning(value: Value) -> Result<Reasoning> {
    let mut reasoning = map(value)?;

    Ok(Reasoning {
        content: reasoning.required("content", string)?,
        duration: reasoning.required("duration", duration)?,
    })
}
