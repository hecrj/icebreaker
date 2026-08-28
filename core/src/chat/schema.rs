use crate::{Chat, Error};

mod decode;
mod encode;

pub fn decode(json: &str) -> Result<Chat, Error> {
    match decoder::run(serde_json::from_str, decode::chat, json) {
        Ok(chat) => Ok(chat),
        Err(error) => {
            log::warn!("{error:?}");
            Err(error)?
        }
    }
}

pub fn encode(chat: &Chat) -> Result<String, Error> {
    let value = encode::chat(chat.clone());

    Ok(serde_json::to_string_pretty(&value)?)
}
