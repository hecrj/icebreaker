use crate::{Chat, Error};

mod decode;
mod encode;
mod old;

pub fn decode(json: &str) -> Result<Chat, Error> {
    match decoder::run(serde_json::from_str, decode::chat, json) {
        Ok(chat) => {
            return Ok(chat);
        }
        Err(error) => {
            log::warn!("{error:?}");
        }
    }

    let schema: old::Schema = serde_json::from_str(json)?;

    let chat = Chat {
        id: schema.id,
        file: schema.file,
        title: schema.title,
        history: schema
            .history
            .into_iter()
            .map(old::Message::to_data)
            .collect(),
    };

    Ok(chat)
}

pub fn encode(chat: &Chat) -> Result<String, Error> {
    let value = encode::chat(chat.clone());

    Ok(serde_json::to_string_pretty(&value)?)
}
