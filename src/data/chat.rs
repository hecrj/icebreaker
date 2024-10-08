use crate::data::assistant::{self, Assistant, Message};
use crate::data::Error;

use futures::{SinkExt, Stream, StreamExt};
use serde::{Deserialize, Serialize};
use tokio::fs;
use tokio::task;
use uuid::Uuid;

use std::io;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct Chat {
    pub id: Id,
    pub file: assistant::File,
    pub title: Option<String>,
    pub history: Vec<Message>,
}

#[derive(Debug, Serialize, Deserialize)]
struct Schema {
    pub id: Id,
    pub file: assistant::File,
    pub title: Option<String>,
    pub history: Vec<Message>,
}

impl Chat {
    async fn path(id: &Id) -> Result<PathBuf, Error> {
        Ok(storage_dir().await?.join(format!("{}.json", id.0.simple())))
    }

    pub async fn list() -> Result<Vec<Entry>, Error> {
        let list = List::fetch().await?;

        Ok(list.entries)
    }

    pub async fn fetch(id: Id) -> Result<Self, Error> {
        let bytes = fs::read(Self::path(&id).await?).await?;
        let schema: Schema = task::spawn_blocking(move || serde_json::from_slice(&bytes)).await??;

        Ok(Self {
            id,
            file: schema.file,
            title: schema.title,
            history: schema.history,
        })
    }

    pub async fn fetch_last_opened() -> Result<Self, Error> {
        let LastOpened(id) = LastOpened::fetch().await?;

        Self::fetch(id).await
    }

    pub async fn create(
        file: assistant::File,
        title: Option<String>,
        history: Vec<Message>,
    ) -> Result<Self, Error> {
        let id = Id(Uuid::new_v4());
        let chat = Self::save(id, file, title, history).await?;

        LastOpened::update(chat.id.clone()).await?;

        List::push(Entry {
            id: chat.id.clone(),
            file: chat.file.clone(),
            title: chat.title.clone(),
        })
        .await?;

        Ok(chat)
    }

    pub async fn save(
        id: Id,
        file: assistant::File,
        title: Option<String>,
        history: Vec<Message>,
    ) -> Result<Self, Error> {
        if let Ok(current) = Self::fetch(id.clone()).await {
            if current.title != title {
                let mut list = List::fetch().await?;

                if let Some(entry) = list.entries.iter_mut().find(|entry| entry.id == id) {
                    entry.title = title.clone();
                }

                list.save().await?;
            }
        }

        let chat = Schema {
            id,
            file,
            title,
            history,
        };

        let (bytes, chat) =
            task::spawn_blocking(move || (serde_json::to_vec_pretty(&chat), chat)).await?;

        fs::write(Self::path(&chat.id).await?, bytes?).await?;

        Ok(Self {
            id: chat.id,
            file: chat.file,
            title: chat.title,
            history: chat.history,
        })
    }

    pub async fn delete(id: Id) -> Result<(), Error> {
        fs::remove_file(Self::path(&id).await?).await?;

        let _ = List::remove(&id).await;

        match LastOpened::fetch().await {
            Ok(LastOpened(last_opened)) if id == last_opened => {
                let list = List::fetch().await.ok();

                match list.as_ref().and_then(|list| list.entries.first()) {
                    Some(entry) => {
                        LastOpened::update(entry.id.clone()).await?;
                    }
                    None => {
                        LastOpened::delete().await?;
                    }
                }
            }
            _ => {}
        }

        Ok(())
    }
}

#[derive(Debug, Clone)]
pub enum Event {
    MessageSent(Message),
    MessageAdded(Message),
    LastMessageChanged(Message),
    ExchangeOver,
    TitleChanged(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Content(String);

impl Content {
    pub fn parse(content: &str) -> Option<Self> {
        let content = content.trim();

        if content.is_empty() {
            return None;
        }

        Some(Self(content.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

pub fn send(
    assistant: &Assistant,
    history: &[Message],
    message: Content,
) -> impl Stream<Item = Result<Event, Error>> {
    const SYSTEM_PROMPT: &str = "You are a helpful assistant.";

    let assistant = assistant.clone();
    let mut messages = history.to_vec();
    let message = message.as_str().to_owned();

    iced::stream::try_channel(1, |mut sender| async move {
        messages.push(Message::User(message.clone()));

        let _ = sender
            .send(Event::MessageSent(Message::User(message)))
            .await;

        let _ = sender
            .send(Event::MessageAdded(Message::Assistant(String::new())))
            .await;

        let mut message = String::new();

        {
            let mut next_message = assistant.complete(SYSTEM_PROMPT, &messages).boxed();
            let mut first = false;

            while let Some(token) = next_message.next().await.transpose()? {
                message.push_str(&token);

                let event = if first {
                    first = false;
                    Event::MessageAdded
                } else {
                    Event::LastMessageChanged
                };

                let _ = sender
                    .send(event(Message::Assistant(message.trim().to_owned())))
                    .await;
            }
        }

        // Suggest a title after the 1st and 5th messages
        if messages.len() == 1 || messages.len() == 5 {
            messages.push(Message::Assistant(message.trim().to_owned()));
            messages.push(Message::User(
                "Give me a short title for our conversation so far. \
                    Just the title between quotes; don't say anything else."
                    .to_owned(),
            ));

            let mut title_suggestion = assistant.complete(SYSTEM_PROMPT, &messages).boxed();
            let mut title = String::new();

            while let Some(token) = title_suggestion.next().await.transpose()? {
                title.push_str(&token);

                if title.len() > 80 {
                    title.push_str("...");
                }

                let _ = sender
                    .send(Event::TitleChanged(
                        title.trim().trim_matches('"').to_owned(),
                    ))
                    .await;

                if title.len() > 80 {
                    break;
                }
            }
        }

        let _ = sender.send(Event::ExchangeOver).await;

        Ok(())
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Id(Uuid);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entry {
    pub id: Id,
    pub file: assistant::File,
    pub title: Option<String>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct List {
    entries: Vec<Entry>,
}

impl List {
    async fn path() -> Result<PathBuf, io::Error> {
        Ok(storage_dir().await?.join("list.json"))
    }

    async fn fetch() -> Result<Self, Error> {
        let path = Self::path().await?;

        let bytes = fs::read(&path).await;

        let Ok(bytes) = bytes else {
            return Ok(List::default());
        };

        let list: Self =
            { task::spawn_blocking(move || serde_json::from_slice(&bytes).ok()).await? }
                .unwrap_or_default();

        Ok(list)
    }

    async fn push(entry: Entry) -> Result<(), Error> {
        let mut list = Self::fetch().await.unwrap_or_default();
        list.entries.insert(0, entry);

        list.save().await
    }

    async fn remove(id: &Id) -> Result<(), Error> {
        let mut list = List::fetch().await?;
        list.entries.retain(|entry| &entry.id != id);

        list.save().await
    }

    async fn save(self) -> Result<(), Error> {
        let json = task::spawn_blocking(move || serde_json::to_vec_pretty(&self)).await?;

        fs::write(Self::path().await?, json?).await?;

        Ok(())
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct LastOpened(Id);

impl LastOpened {
    async fn path() -> Result<PathBuf, io::Error> {
        Ok(storage_dir().await?.join("last_opened.json"))
    }

    async fn fetch() -> Result<Self, Error> {
        let path = Self::path().await?;
        let bytes = fs::read(path).await?;

        Ok(serde_json::from_slice(&bytes)?)
    }

    async fn update(id: Id) -> Result<(), Error> {
        let json = serde_json::to_vec(&LastOpened(id))?;

        fs::write(Self::path().await?, json).await?;

        Ok(())
    }

    async fn delete() -> Result<(), Error> {
        fs::remove_file(Self::path().await?).await?;

        Ok(())
    }
}

async fn storage_dir() -> Result<PathBuf, io::Error> {
    let directory = dirs_next::data_local_dir()
        .unwrap_or(PathBuf::from("."))
        .join("icebreaker")
        .join("chats");

    fs::create_dir_all(&directory).await?;

    Ok(directory)
}
