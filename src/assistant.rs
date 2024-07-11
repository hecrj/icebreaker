use futures::{SinkExt, Stream, StreamExt};
use serde::Deserialize;
use serde_json::json;
use tokio::fs;
use tokio::io::{self, AsyncBufReadExt, AsyncWriteExt};
use tokio::process;

use std::fmt;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct Assistant {
    model: Id,
    _container: Arc<Container>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backend {
    CPU,
    CUDA,
}

impl Assistant {
    const LLAMA_CPP_CONTAINER_CPU: &'static str = "ghcr.io/ggerganov/llama.cpp:server--b1-a59f8fd";
    const LLAMA_CPP_CONTAINER_CUDA: &'static str =
        "ghcr.io/ggerganov/llama.cpp:server-cuda--b1-a59f8fd";

    const MODELS_DIR: &'static str = "./models";
    const HOST_PORT: u64 = 8080;

    pub fn boot(file: File, backend: Backend) -> impl Stream<Item = Result<BootEvent, Error>> {
        iced::stream::try_channel(1, move |mut sender| async move {
            let _ = fs::create_dir_all(Self::MODELS_DIR).await?;

            let model_path = format!(
                "{directory}/{filename}",
                directory = Self::MODELS_DIR,
                filename = file.name
            );

            if !fs::try_exists(&model_path).await? {
                let mut model = fs::File::create(&model_path).await?;

                let mut download = {
                    let url = format!(
                        "https://huggingface.co\
                            /{id}/resolve/main/\
                            {filename}?download=true",
                        id = file.model.0,
                        filename = file.name
                    );

                    reqwest::get(url)
                }
                .await?;

                let model_size = download.content_length();
                let mut downloaded = 0;
                let mut progress = 0;

                while let Some(chunk) = download.chunk().await? {
                    downloaded += chunk.len() as u64;

                    if let Some(model_size) = model_size {
                        let new_progress =
                            (100.0 * downloaded as f32 / model_size as f32).round() as u64;

                        if new_progress > progress {
                            progress = new_progress;

                            let _ = sender
                                .send(BootEvent::Logged(format!(
                                    "Downloading {file}... {progress}%",
                                    file = file.name,
                                )))
                                .await;

                            let _ = sender
                                .send(BootEvent::Progressed {
                                    percent: progress / 2,
                                })
                                .await;
                        }
                    }

                    model.write(&chunk).await?;
                }

                model.flush().await?;
            }

            let _ = sender.send(BootEvent::Progressed { percent: 50 }).await;

            let _ = sender
                .send(BootEvent::Logged(format!(
                    "Launching {model} with llama.cpp...",
                    model = file.model.name(),
                )))
                .await;

            let command = match backend {
                Backend::CPU => {
                    format!(
                        "create --rm -p {port}:80 -v {volume}:/models \
                            {container} --model models/{filename} --conversation \
                            --port 80 --host 0.0.0.0",
                        filename = file.name,
                        container = Self::LLAMA_CPP_CONTAINER_CPU,
                        port = Self::HOST_PORT,
                        volume = Self::MODELS_DIR,
                    )
                }
                Backend::CUDA => {
                    format!(
                        "create --rm --gpus all -p {port}:80 -v {volume}:/models \
                            {container} --model models/{filename} --conversation \
                            --port 80 --host 0.0.0.0 --gpu-layers 40",
                        filename = file.name,
                        container = Self::LLAMA_CPP_CONTAINER_CUDA,
                        port = Self::HOST_PORT,
                        volume = Self::MODELS_DIR,
                    )
                }
            };

            let mut docker = process::Command::new("docker")
                .args(
                    command
                        .split(' ')
                        .map(str::trim)
                        .filter(|arg| !arg.is_empty()),
                )
                .kill_on_drop(true)
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()?;

            let notify_progress = {
                let mut sender = sender.clone();

                let output = io::BufReader::new(docker.stderr.take().expect("piped stderr"));

                async move {
                    let mut lines = output.lines();

                    while let Ok(Some(log)) = lines.next_line().await {
                        let _ = sender.send(BootEvent::Logged(log)).await;
                    }
                }
            };

            let _ = tokio::task::spawn(notify_progress);

            let container = {
                let output = io::BufReader::new(docker.stdout.take().expect("piped stdout"));

                let mut lines = output.lines();

                lines
                    .next_line()
                    .await?
                    .ok_or_else(|| Error::DockerFailed("no container id returned by docker"))?
            };

            if !docker.wait().await?.success() {
                return Err(Error::DockerFailed("failed to create container"));
            }

            let _ = sender.send(BootEvent::Progressed { percent: 75 }).await;

            let assistant = Self {
                model: file.model,
                _container: Arc::new(Container {
                    id: container.clone(),
                }),
            };

            let _start = process::Command::new("docker")
                .args(&["start", &container])
                .output()
                .await?;

            let mut logs = process::Command::new("docker")
                .args(&["logs", "-f", &container])
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()?;

            let _ = sender.send(BootEvent::Progressed { percent: 99 }).await;

            let mut lines = {
                use futures::stream;
                use tokio_stream::wrappers::LinesStream;

                let stdout = io::BufReader::new(logs.stdout.take().expect("piped stdout"));

                let stderr = io::BufReader::new(logs.stderr.take().expect("piped stderr"));

                stream::select(
                    LinesStream::new(stdout.lines()),
                    LinesStream::new(stderr.lines()),
                )
            };

            while let Some(log) = lines.next().await.transpose()? {
                let message = log
                    .split("\u{1b}")
                    .last()
                    .map(|log| log.chars().skip(4))
                    .unwrap_or(log.chars().skip(0));

                let _ = sender.send(BootEvent::Logged(message.collect())).await;

                if log.contains("HTTP server listening") {
                    let _ = sender.send(BootEvent::Finished(assistant)).await;

                    return Ok(());
                }
            }

            Err(Error::DockerFailed("container stopped unexpectedly"))
        })
    }

    pub fn chat(
        &self,
        history: &[Message],
        message: &str,
    ) -> impl Stream<Item = Result<ChatEvent, ChatError>> {
        let history = history.to_vec();
        let message = message.to_owned();

        iced::stream::try_channel(1, |mut sender| async move {
            let client = reqwest::Client::new();

            let request = {
                let messages: Vec<_> = history
                    .iter()
                    .map(|message| match message {
                        Message::Assistant(content) => ("assistant", content.as_str()),
                        Message::User(content) => ("user", content.as_str()),
                    })
                    .chain([("user", message.as_str())])
                    .map(|(role, content)| {
                        json!({
                            "role": role,
                            "content": content
                        })
                    })
                    .collect();

                client
                    .post(format!(
                        "http://localhost:{port}/v1/chat/completions",
                        port = Self::HOST_PORT
                    ))
                    .json(&json!({
                        "model": "tgi",
                        "messages": messages,
                        "stream": true,
                    }))
            };

            let mut response = request.send().await?.error_for_status()?;

            let _ = sender
                .send(ChatEvent::MessageSent(Message::User(message)))
                .await;

            let _ = sender
                .send(ChatEvent::MessageAdded(Message::Assistant(String::new())))
                .await;

            let mut message = String::new();
            let mut buffer = Vec::new();

            while let Some(chunk) = response.chunk().await? {
                buffer.extend(chunk);

                let mut lines = buffer
                    .split(|byte| *byte == 0x0A)
                    .filter(|bytes| !bytes.is_empty());

                let last_line = if buffer.ends_with(&[0x0A]) {
                    &[]
                } else {
                    lines.next_back().unwrap_or_default()
                };

                for line in lines {
                    if let Ok(data) = std::str::from_utf8(line) {
                        #[derive(Deserialize)]
                        struct Data {
                            choices: Vec<Choice>,
                        }

                        #[derive(Deserialize)]
                        struct Choice {
                            delta: Delta,
                        }

                        #[derive(Deserialize)]
                        struct Delta {
                            content: Option<String>,
                        }

                        let data: Data = serde_json::from_str(
                            data.trim().strip_prefix("data: ").unwrap_or(data),
                        )?;

                        if let Some(choice) = data.choices.first() {
                            if let Some(content) = &choice.delta.content {
                                message.push_str(content);
                            }
                        }

                        let _ = sender
                            .send(ChatEvent::LastMessageChanged(Message::Assistant(
                                message.trim().to_owned(),
                            )))
                            .await;
                    };
                }

                buffer = last_line.to_vec();
            }

            let _ = sender.send(ChatEvent::ExchangeOver).await;

            Ok(())
        })
    }

    pub fn name(&self) -> &str {
        self.model.name()
    }
}

#[derive(Debug, Clone)]
pub enum Message {
    Assistant(String),
    User(String),
}

impl Message {
    pub fn content(&self) -> &str {
        match self {
            Message::Assistant(content) | Message::User(content) => content,
        }
    }
}

#[derive(Debug, Clone)]
pub enum ChatEvent {
    MessageSent(Message),
    MessageAdded(Message),
    LastMessageChanged(Message),
    ExchangeOver,
}

#[derive(Debug, Clone, thiserror::Error)]
pub enum ChatError {
    #[error("request failed: {0}")]
    RequestFailed(Arc<reqwest::Error>),

    #[error("deserialization failed: {0}")]
    DecodingFailed(Arc<serde_json::Error>),
}

impl From<reqwest::Error> for ChatError {
    fn from(error: reqwest::Error) -> Self {
        Self::RequestFailed(Arc::new(error))
    }
}

impl From<serde_json::Error> for ChatError {
    fn from(error: serde_json::Error) -> Self {
        Self::DecodingFailed(Arc::new(error))
    }
}

#[derive(Debug, Clone)]
struct Container {
    id: String,
}

impl Drop for Container {
    fn drop(&mut self) {
        use std::process;

        let _ = process::Command::new("docker")
            .args(&["stop", &self.id])
            .stdin(process::Stdio::null())
            .stdout(process::Stdio::null())
            .stderr(process::Stdio::null())
            .spawn();
    }
}

#[derive(Debug, Clone)]
pub enum BootEvent {
    Progressed { percent: u64 },
    Logged(String),
    Finished(Assistant),
}

#[derive(Debug, Clone)]
pub struct Model {
    id: Id,
    pub last_modified: chrono::DateTime<chrono::Local>,
    pub downloads: Downloads,
    pub likes: Likes,
    pub files: Vec<File>,
}

impl Model {
    const API_URL: &'static str = "https://huggingface.co/api";

    pub async fn list() -> Result<Vec<Self>, Error> {
        Self::search(String::new()).await
    }

    pub async fn search(query: String) -> Result<Vec<Self>, Error> {
        let client = reqwest::Client::new();

        let request = client.get(format!("{}/models", Self::API_URL)).query(&[
            ("search", query.as_ref()),
            ("filter", "text-generation-inference"),
            ("filter", "gguf"),
            ("sort", "downloads"),
            ("limit", "100"),
            ("full", "true"),
        ]);

        #[derive(Deserialize)]
        struct Response {
            id: Id,
            #[serde(rename = "lastModified")]
            last_modified: chrono::DateTime<chrono::Local>,
            downloads: Downloads,
            likes: Likes,
            gated: Gated,
            siblings: Vec<Sibling>,
        }

        #[derive(Deserialize, PartialEq, Eq)]
        #[serde(untagged)]
        enum Gated {
            Bool(bool),
            Other(String),
        }

        #[derive(Deserialize)]
        struct Sibling {
            rfilename: String,
        }

        let response = request.send().await?;
        let mut models: Vec<Response> = response.json().await?;

        models.retain(|model| model.gated == Gated::Bool(false));

        Ok(models
            .into_iter()
            .map(|model| Self {
                id: model.id.clone(),
                last_modified: model.last_modified,
                downloads: model.downloads,
                likes: model.likes,
                files: model
                    .siblings
                    .into_iter()
                    .filter(|file| file.rfilename.ends_with(".gguf"))
                    .map(|file| File {
                        model: model.id.clone(),
                        name: file.rfilename,
                    })
                    .collect(),
            })
            .collect())
    }

    pub fn name(&self) -> &str {
        self.id.name()
    }

    pub fn author(&self) -> &str {
        self.id.author()
    }
}

impl fmt::Display for Model {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.id.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
struct Id(String);

impl Id {
    pub fn name(&self) -> &str {
        self.0
            .split_once('/')
            .map(|(_author, name)| name)
            .unwrap_or(&self.0)
    }

    pub fn author(&self) -> &str {
        self.0
            .split_once('/')
            .map(|(author, _name)| author)
            .unwrap_or(&self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub struct Downloads(u64);

impl fmt::Display for Downloads {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            1_000_000.. => {
                write!(f, "{:.2}M", (self.0 as f32 / 1_000_000 as f32))
            }
            1_000.. => {
                write!(f, "{:.2}k", (self.0 as f32 / 1_000 as f32))
            }
            _ => {
                write!(f, "{}", self.0)
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub struct Likes(u64);

impl fmt::Display for Likes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone)]
pub struct File {
    model: Id,
    pub name: String,
}

#[derive(Debug, Clone, thiserror::Error)]
pub enum Error {
    #[error("request failed: {0}")]
    RequestFailed(Arc<reqwest::Error>),
    #[error("io operation failed: {0}")]
    IOFailed(Arc<io::Error>),
    #[error("docker operation failed: {0}")]
    DockerFailed(&'static str),
}

impl From<reqwest::Error> for Error {
    fn from(error: reqwest::Error) -> Self {
        Self::RequestFailed(Arc::new(error))
    }
}

impl From<io::Error> for Error {
    fn from(error: io::Error) -> Self {
        Self::IOFailed(Arc::new(error))
    }
}
