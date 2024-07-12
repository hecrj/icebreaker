use futures::channel::mpsc;
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
    _server: Arc<Server>,
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
        #[derive(Clone)]
        struct Sender(mpsc::Sender<BootEvent>);

        impl Sender {
            async fn log(&mut self, log: String) {
                let _ = self.0.send(BootEvent::Logged(log)).await;
            }

            async fn progress(&mut self, stage: &'static str, percent: u64) {
                let _ = self.0.send(BootEvent::Progressed { stage, percent }).await;
            }

            async fn finish(mut self, assistant: Assistant) {
                let _ = self.0.send(BootEvent::Finished(assistant)).await;
            }
        }

        iced::stream::try_channel(1, move |sender| async move {
            let mut sender = Sender(sender);
            let _ = fs::create_dir_all(Self::MODELS_DIR).await?;

            let model_path = format!(
                "{directory}/{filename}",
                directory = Self::MODELS_DIR,
                filename = file.name
            );

            if fs::try_exists(&model_path).await? {
                sender.progress("Verifying model...", 0).await;
                sender
                    .log(format!(
                        "{filename} found. Verifying...",
                        filename = file.name
                    ))
                    .await;

                let metadata = reqwest::get(format!(
                    "https://huggingface.co/{model}/raw/main/{filename}",
                    model = file.model.0,
                    filename = file.name
                ))
                .await?
                .text()
                .await?;

                let size: u64 = metadata
                    .lines()
                    .next_back()
                    .unwrap_or_default()
                    .split_whitespace()
                    .next_back()
                    .unwrap_or_default()
                    .parse()
                    .unwrap_or_default();

                let file_metadata = fs::metadata(&model_path).await?;

                if size == file_metadata.len() {
                    sender.log(format!("File sizes match! {size} bytes")).await;
                } else {
                    sender
                        .log(format!(
                            "Invalid file size. Deleting {filename}...",
                            filename = file.name
                        ))
                        .await;

                    fs::remove_file(&model_path).await?;
                }
            }

            if !fs::try_exists(&model_path).await? {
                sender
                    .log(format!(
                        "{filename} not found. Starting download...",
                        filename = file.name
                    ))
                    .await;

                let mut model = io::BufWriter::new(fs::File::create(&model_path).await?);

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

                            sender
                                .log(format!(
                                    "Downloading {file}... {progress}%",
                                    file = file.name,
                                ))
                                .await;

                            sender.progress("Downloading model...", progress).await;
                        }
                    }

                    model.write(&chunk).await?;
                }

                model.flush().await?;
            }

            sender.progress("Detecting executor...", 0).await;

            let (server, stdout, stderr) = if let Ok(version) =
                process::Command::new("llama-server")
                    .arg("--version")
                    .output()
                    .await
            {
                sender
                    .log("Local llama-server binary found!".to_owned())
                    .await;

                let mut lines = version.stdout.lines();

                while let Some(line) = lines.next_line().await? {
                    sender.log(line).await;
                }

                sender
                    .log(format!(
                        "Launching {model} with local llama-server...",
                        model = file.model.name(),
                    ))
                    .await;

                let mut server = Self::launch_with_executable("llama-server", &file, backend)?;
                let stdout = server.stdout.take();
                let stderr = server.stderr.take();

                (Server::Process(server), stdout, stderr)
            } else if let Ok(_docker) = process::Command::new("docker")
                .arg("version")
                .output()
                .await
            {
                sender
                    .log(format!(
                        "Launching {model} with Docker...",
                        model = file.model.name(),
                    ))
                    .await;

                sender.progress("Preparing container...", 0).await;

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
                    .args(Self::parse_args(&command))
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
                            sender.log(log).await;
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

                sender.progress("Launching assistant...", 99).await;

                let server = Server::Container(container.clone());

                let _start = process::Command::new("docker")
                    .args(&["start", &container])
                    .output()
                    .await?;

                let mut logs = process::Command::new("docker")
                    .args(&["logs", "-f", &container])
                    .stdout(std::process::Stdio::piped())
                    .stderr(std::process::Stdio::piped())
                    .spawn()?;

                (server, logs.stdout.take(), logs.stderr.take())
            } else {
                return Err(Error::NoExecutorAvailable);
            };

            let mut lines = {
                use futures::stream;
                use tokio_stream::wrappers::LinesStream;

                let stdout = io::BufReader::new(stdout.expect("piped stdout"));
                let stderr = io::BufReader::new(stderr.expect("piped stderr"));

                stream::select(
                    LinesStream::new(stdout.lines()),
                    LinesStream::new(stderr.lines()),
                )
            };

            while let Some(log) = lines.next().await.transpose()? {
                if log.contains("HTTP server listening") {
                    sender
                        .finish(Assistant {
                            model: file.model.clone(),
                            _server: Arc::new(server),
                        })
                        .await;

                    return Ok(());
                }

                sender.log(log).await;
            }

            Err(Error::ExecutorFailed("llama-server exited unexpectedly"))
        })
    }

    pub fn chat(
        &self,
        history: &[Message],
        message: &str,
    ) -> impl Stream<Item = Result<ChatEvent, ChatError>> {
        let model = self.model.clone();
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
                        "model": format!("{model}", model = model.name()),
                        "messages": messages,
                        "stream": true,
                        "cache_prompt": true,
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

    fn launch_with_executable(
        executable: &'static str,
        file: &File,
        backend: Backend,
    ) -> Result<process::Child, Error> {
        let gpu_flags = match backend {
            Backend::CPU => "",
            Backend::CUDA => "--gpu-layers 40",
        };

        let server = process::Command::new(executable)
            .args(Self::parse_args(&format!(
                "--model models/{filename} --conversation \
                    --port 8080 --host 0.0.0.0 {gpu_flags}",
                filename = file.name,
            )))
            .kill_on_drop(true)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()?;

        Ok(server)
    }

    fn parse_args(command: &str) -> impl Iterator<Item = &str> {
        command
            .split(' ')
            .map(str::trim)
            .filter(|arg| !arg.is_empty())
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

#[derive(Debug)]
enum Server {
    Container(String),
    Process(process::Child),
}

impl Drop for Server {
    fn drop(&mut self) {
        use std::process;

        match self {
            Self::Container(id) => {
                let _ = process::Command::new("docker")
                    .args(&["stop", id])
                    .stdin(process::Stdio::null())
                    .stdout(process::Stdio::null())
                    .stderr(process::Stdio::null())
                    .spawn();
            }
            Self::Process(_process) => {}
        }
    }
}

#[derive(Debug, Clone)]
pub enum BootEvent {
    Progressed { stage: &'static str, percent: u64 },
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
    #[error("executor failed: {0}")]
    ExecutorFailed(&'static str),
    #[error("no suitable executor was found: neither llama-server nor docker are installed")]
    NoExecutorAvailable,
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
