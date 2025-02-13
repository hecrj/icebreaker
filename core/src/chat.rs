mod schema;

use crate::assistant::{self, Assistant, Reply, Token};
use crate::chat::schema::Schema;
use crate::model;
use crate::plan::{self, Plan};
use crate::Error;

use serde::{Deserialize, Serialize};
use sipper::{sipper, Sipper, Straw};
use tokio::fs;
use tokio::task;
use uuid::Uuid;

use std::fmt;
use std::io;
use std::path::PathBuf;

#[derive(Clone)]
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

        let _ = LastOpened::update(id).await;

        task::spawn_blocking(move || {
            let schema: Schema = serde_json::from_slice(&bytes)?;

            let history = schema
                .history
                .into_iter()
                .map(schema::Message::to_data)
                .collect();

            Ok(Self {
                id,
                file: schema.file,
                title: schema.title,
                history,
            })
        })
        .await?
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

        let (bytes, chat, history) = task::spawn_blocking(move || {
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

            (serde_json::to_vec_pretty(&chat), chat, history)
        })
        .await?;

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

impl fmt::Debug for Chat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Chat")
            .field("id", &self.id)
            .field("file", &self.file)
            .field("title", &self.title)
            .finish()
    }
}

#[derive(Debug, Clone)]
pub enum Event {
    ReplyAdded,
    ReplyChanged { reply: Reply, new_token: Token },
    PlanAdded,
    PlanChanged(plan::Event),
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
) -> impl Straw<(), Event, Error> {
    let assistant = assistant.clone();
    let history = history(items);

    sipper(move |mut sender| async move {
        if strategy.search {
            let _ = sender.send(Event::PlanAdded).await;

            Plan::search(&assistant, &history)
                .with(Event::PlanChanged)
                .run(&sender)
                .await?;
        } else {
            reply(&assistant, &history).run(sender).await?;
        }

        Ok(())
    })
}

fn reply<'a>(
    assistant: &'a Assistant,
    messages: &'a [assistant::Message],
) -> impl Straw<(), Event, Error> + 'a {
    sipper(move |mut sender| async move {
        let _ = sender.send(Event::ReplyAdded).await;

        let _reply = assistant
            .reply(SYSTEM_PROMPT, messages, &[])
            .with(|(reply, new_token)| Event::ReplyChanged { reply, new_token })
            .run(sender)
            .await;

        Ok(())
    })
}

pub fn title(assistant: &Assistant, items: &[Item]) -> impl Straw<String, String, Error> {
    let assistant = assistant.clone();
    let history = history(&items);

    sipper(move |mut sender| async move {
        let request = [assistant::Message::User(
            "Give me a short title for our conversation so far, \
                    without considering this interaction. \
                    Just the title between quotes; don't say anything else."
                .to_owned(),
        )];

        let mut title = String::new();

        fn sanitize(title: &str) -> String {
            title.trim().trim_matches('"').to_owned()
        }

        let mut completion = assistant.complete(SYSTEM_PROMPT, &history, &request).pin();

        while let Some(token) = completion.sip().await {
            if let Token::Talking(token) = token {
                title.push_str(&token);
            }

            let is_too_long = title.len() > 80;

            if is_too_long {
                title.push_str("...");
            }

            sender.send(sanitize(&title)).await;
        }

        Ok(sanitize(&title))
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
    let directory = dirs::data_local_dir()
        .unwrap_or(PathBuf::from("."))
        .join("icebreaker")
        .join("chats");

    fs::create_dir_all(&directory).await?;

    Ok(directory)
}

fn history(items: &[Item]) -> Vec<assistant::Message> {
    items
        .iter()
        .flat_map(|item| match item {
            Item::User(query) => vec![assistant::Message::User(query.clone())],
            Item::Reply(reply) => vec![assistant::Message::Assistant(reply.content.clone())],
            Item::Plan(plan) => plan
                .answers()
                .map(|reply| assistant::Message::Assistant(reply.content.clone()))
                .collect(),
        })
        .collect()
}
