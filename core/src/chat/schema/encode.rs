use crate::Chat;
use crate::assistant::{Reasoning, Reply};
use crate::chat;

use decoder::encode::{duration, map, optional, sequence, string};
use decoder::{Map, Value};

pub fn chat(chat: Chat) -> Value {
    map([
        ("id", chat.id.encode()),
        ("file", chat.file.encode()),
        ("title", optional(string, chat.title)),
        ("history", sequence(item, chat.history)),
    ])
    .into()
}

fn item(item: chat::Item) -> Map {
    let (type_, item) = match item {
        chat::Item::User(message) => ("user", map([("message", string(message))])),
        chat::Item::Reply(reply_) => ("reply", reply(reply_)),
    };

    item.tag("type", type_)
}

fn reply(reply: Reply) -> Map {
    map([
        ("reasoning", optional(reasoning, reply.reasoning)),
        ("content", string(reply.content)),
    ])
}

fn reasoning(reasoning: Reasoning) -> Map {
    map([
        ("content", string(reasoning.content)),
        ("duration", duration(reasoning.duration)),
    ])
}
