mod schema;

use crate::data::assistant::{self, Assistant, Reply, Token};
use crate::data::chat::schema::Schema;
use crate::data::model;
use crate::data::plan::{self, Plan};
use crate::data::Error;

use futures::{SinkExt, Stream, StreamExt};
use serde::{Deserialize, Serialize};
use sipper::Sender;
use tokio::fs;
use tokio::task;
use uuid::Uuid;

use std::io;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct Chat {
    pub id: Id,
    pub file: model::File,
    pub title: Option<String>,
    pub history: Vec<Item>,
}

#[derive(Debug, Clone)]
pub enum Item {
    User(String),
    Reply(Reply),
    Plan(Plan),
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

        let _ = LastOpened::update(id).await;

        Ok(Self {
            id,
            file: schema.file,
            title: schema.title,
            history: schema
                .history
                .into_iter()
                .map(schema::Message::to_data)
                .collect(),
        })
    }

    pub async fn fetch_last_opened() -> Result<Self, Error> {
        let LastOpened(id) = LastOpened::fetch().await?;

        Self::fetch(id).await
    }

    pub async fn create(
        file: model::File,
        title: Option<String>,
        history: Vec<Item>,
    ) -> Result<Self, Error> {
        let id = Id(Uuid::new_v4());
        let chat = Self::save(id, file, title, history).await?;

        LastOpened::update(chat.id).await?;

        List::push(Entry {
            id: chat.id,
            file: chat.file.clone(),
            title: chat.title.clone(),
        })
        .await?;

        Ok(chat)
    }

    pub async fn save(
        id: Id,
        file: model::File,
        title: Option<String>,
        history: Vec<Item>,
    ) -> Result<Self, Error> {
        if let Ok(current) = Self::fetch(id).await {
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
            history: history
                .iter()
                .cloned()
                .map(schema::Message::from_data)
                .collect(),
        };

        let (bytes, chat) =
            task::spawn_blocking(move || (serde_json::to_vec_pretty(&chat), chat)).await?;

        fs::write(Self::path(&chat.id).await?, bytes?).await?;

        Ok(Self {
            id: chat.id,
            file: chat.file,
            title: chat.title,
            history,
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
                        LastOpened::update(entry.id).await?;
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
    ReplyAdded,
    ReplyChanged { reply: Reply, new_token: Token },
    PlanAdded,
    PlanChanged(plan::Event),
    ExchangeOver,
}

const SYSTEM_PROMPT: &str = "You are a helpful assistant.";

#[derive(Debug, Clone, Copy, Default)]
pub struct Strategy {
    pub search: bool,
}

pub fn complete(
    assistant: &Assistant,
    items: &[Item],
    strategy: Strategy,
) -> impl Stream<Item = Result<Event, Error>> {
    let assistant = assistant.clone();
    let history = history(items);

    // TODO
    iced::stream::try_channel(1, move |sender| async move {
        let mut sender = Sender::new(sender);

        if strategy.search {
            let _ = sender.send(Event::PlanAdded).await;

            Plan::search(&assistant, &history, &mut sender.map(Event::PlanChanged)).await?
        } else {
            reply(&assistant, &history, &mut sender).await?;
        }

        sender.send(Event::ExchangeOver).await;

        Ok(())
    })
}

async fn reply<'a>(
    assistant: &'a Assistant,
    messages: &'a [assistant::Message],
    sender: &mut Sender<Event>,
) -> Result<(), Error> {
    let _ = sender.send(Event::ReplyAdded).await;

    let _reply = assistant
        .reply(
            SYSTEM_PROMPT,
            messages,
            &[],
            &mut sender.map(|(reply, new_token)| Event::ReplyChanged { reply, new_token }),
        )
        .await;

    Ok(())
}

#[derive(Debug, Clone)]
pub enum Title {
    Partial(String),
    Complete(String),
}

pub fn title(assistant: &Assistant, items: &[Item]) -> impl Stream<Item = Result<Title, Error>> {
    let assistant = assistant.clone();
    let history = history(&items);

    iced::stream::try_channel(1, move |mut sender| async move {
        let request = [assistant::Message::User(
            "Give me a short title for our conversation so far, \
                    without considering this interaction. \
                    Just the title between quotes; don't say anything else."
                .to_owned(),
        )];

        let mut title_suggestion = assistant
            .complete(SYSTEM_PROMPT, &history, &request)
            .boxed();

        let mut title = String::new();

        while let Some(token) = title_suggestion.next().await.transpose()? {
            if let Token::Talking(token) = token {
                title.push_str(&token);

                if title.len() > 80 {
                    title.push_str("...");
                }

                let _ = sender
                    .send(Title::Partial(title.trim().trim_matches('"').to_owned()))
                    .await;

                if title.len() > 80 {
                    break;
                }
            }
        }

        let _ = sender
            .send(Title::Complete(title.trim().trim_matches('"').to_owned()))
            .await;

        Ok(())
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Id(Uuid);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entry {
    pub id: Id,
    pub file: model::File,
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

fn history(items: &[Item]) -> Vec<assistant::Message> {
    items
        .iter()
        .filter_map(|item| match item {
            Item::User(query) => Some(assistant::Message::User(query.clone())),
            Item::Reply(reply) => Some(assistant::Message::Assistant(reply.content.clone())),
            Item::Plan(_plan) => None,
        })
        .collect()
}
