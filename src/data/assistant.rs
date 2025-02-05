use crate::data::model;
use crate::data::Error;

use futures::channel::mpsc;
use futures::{FutureExt, SinkExt, Stream, StreamExt};
use serde::Deserialize;
use serde_json::json;
use sipper::Sender;
use tokio::fs;
use tokio::io::{self, AsyncBufReadExt, AsyncWriteExt};
use tokio::process;

use std::sync::Arc;
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub struct Assistant {
    file: model::File,
    _server: Arc<Server>,
}

impl Assistant {
    const LLAMA_CPP_CONTAINER_CPU: &'static str = "ghcr.io/ggerganov/llama.cpp:server-b4600";
    const LLAMA_CPP_CONTAINER_CUDA: &'static str = "ghcr.io/ggerganov/llama.cpp:server-cuda-b4600";
    const LLAMA_CPP_CONTAINER_ROCM: &'static str = "ghcr.io/hecrj/icebreaker:server-rocm-b4600";

    const MODELS_DIR: &'static str = "./models";
    const HOST_PORT: u64 = 8080;

    pub fn boot(
        file: model::File,
        backend: Backend,
    ) -> impl Stream<Item = Result<BootEvent, Error>> {
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

            fs::create_dir_all(Self::MODELS_DIR).await?;

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
                let start = Instant::now();

                sender
                    .log(format!("Downloading {file}...", file = file.name))
                    .await;

                while let Some(chunk) = download.chunk().await? {
                    downloaded += chunk.len() as u64;

                    let speed = downloaded as f32 / start.elapsed().as_secs_f32();

                    if let Some(model_size) = model_size {
                        let new_progress =
                            (100.0 * downloaded as f32 / model_size as f32).round() as u64;

                        if new_progress > progress {
                            progress = new_progress;

                            sender.progress("Downloading model...", progress).await;

                            if progress % 5 == 0 {
                                sender
                                .log(format!(
                                    "=> {progress}% {downloaded:.2}GB of {model_size:.2}GB @ {speed:.2} MB/s",
                                    downloaded = downloaded as f32 / 10f32.powi(9),
                                    model_size = model_size as f32 / 10f32.powi(9),
                                    speed = speed / 10f32.powi(6),
                                ))
                                .await;
                            }
                        }
                    }

                    model.write_all(&chunk).await?;
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

                sender.progress("Launching assistant...", 99).await;

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
                    Backend::Cpu => {
                        format!(
                            "create --rm -p {port}:80 -v {volume}:/models \
                            {container} --model /models/{filename} \
                            --port 80 --host 0.0.0.0",
                            filename = file.name,
                            container = Self::LLAMA_CPP_CONTAINER_CPU,
                            port = Self::HOST_PORT,
                            volume = Self::MODELS_DIR,
                        )
                    }
                    Backend::Cuda => {
                        format!(
                            "create --rm --gpus all -p {port}:80 -v {volume}:/models \
                            {container} --model /models/{filename} \
                            --port 80 --host 0.0.0.0 --gpu-layers 40",
                            filename = file.name,
                            container = Self::LLAMA_CPP_CONTAINER_CUDA,
                            port = Self::HOST_PORT,
                            volume = Self::MODELS_DIR,
                        )
                    }
                    Backend::Rocm => {
                        format!(
                            "create --rm -p {port}:80 -v {volume}:/models \
                            --device=/dev/kfd --device=/dev/dri \
                            --security-opt seccomp=unconfined --group-add video \
                            {container} --model /models/{filename} \
                            --port 80 --host 0.0.0.0 --gpu-layers 40",
                            filename = file.name,
                            container = Self::LLAMA_CPP_CONTAINER_ROCM,
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

                let _handle = tokio::task::spawn(notify_progress);

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
                    .args(["start", &container])
                    .output()
                    .await?;

                let mut logs = process::Command::new("docker")
                    .args(["logs", "-f", &container])
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

            let log_output = {
                let mut sender = sender.clone();

                async move {
                    while let Some(line) = lines.next().await {
                        if let Ok(log) = line {
                            sender.log(log).await;
                        }
                    }

                    return false;
                }
                .boxed()
            };

            let check_health = async move {
                loop {
                    tokio::time::sleep(Duration::from_secs(1)).await;

                    if let Ok(response) = reqwest::get(format!(
                        "http://localhost:{port}/health",
                        port = Self::HOST_PORT
                    ))
                    .await
                    {
                        if response.error_for_status().is_ok() {
                            return true;
                        }
                    }
                }
            }
            .boxed();

            if futures::future::select(log_output, check_health)
                .await
                .factor_first()
                .0
            {
                sender
                    .finish(Assistant {
                        file,
                        _server: Arc::new(server),
                    })
                    .await;

                return Ok(());
            }

            Err(Error::ExecutorFailed("llama-server exited unexpectedly"))
        })
    }

    pub async fn reply(
        &self,
        prompt: &str,
        messages: &[Message],
        append: &[Message],
        sender: &mut Sender<(Reply, Token)>,
    ) -> Result<Reply, Error> {
        let mut reasoning = None;
        let mut reasoning_started_at: Option<Instant> = None;
        let mut content = String::new();
        let mut reasoning_content = String::new();
        let mut next_token = self.complete(prompt, messages, append).boxed();

        while let Some(token) = next_token.next().await.transpose()? {
            match &token {
                Token::Reasoning(token) => {
                    reasoning = {
                        let mut reasoning = reasoning.take().unwrap_or_else(|| Reasoning {
                            content: String::new(),
                            duration: Duration::ZERO,
                        });

                        if let Some(reasoning_started_at) = reasoning_started_at {
                            reasoning.duration = reasoning_started_at.elapsed();
                        } else {
                            reasoning_started_at = Some(Instant::now());
                        }

                        reasoning_content.push_str(token);
                        reasoning.content = reasoning_content.trim().to_owned();

                        Some(reasoning)
                    };
                }
                Token::Talking(token) => {
                    content.push_str(token);
                }
            }

            let _ = sender
                .send((
                    Reply {
                        reasoning: reasoning.clone(),
                        content: content.trim().to_owned(),
                    },
                    token,
                ))
                .await;
        }

        Ok(Reply {
            reasoning: reasoning.clone(),
            content: content.trim().to_owned(),
        })
    }

    pub fn complete<'a>(
        &'a self,
        system_prompt: &'a str,
        messages: &'a [Message],
        append: &'a [Message],
    ) -> impl Stream<Item = Result<Token, Error>> + 'a {
        iced::stream::try_channel(1, move |mut sender| async move {
            let client = reqwest::Client::new();

            let request = {
                let messages: Vec<_> = [("system", system_prompt)]
                    .into_iter()
                    .chain(messages.iter().chain(append).map(Message::to_tuple))
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
                        "model": format!("{model}", model = self.name()),
                        "messages": messages,
                        "stream": true,
                        "cache_prompt": true,
                    }))
            };

            let mut response = request.send().await?.error_for_status()?;
            let mut buffer = Vec::new();
            let mut is_reasoning = None;

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

                        if data == "data: [DONE]" {
                            break;
                        }

                        let mut data: Data = serde_json::from_str(
                            data.trim().strip_prefix("data: ").unwrap_or(data),
                        )?;

                        if let Some(choice) = data.choices.first_mut() {
                            if let Some(content) = &mut choice.delta.content {
                                match is_reasoning {
                                    None if content.contains("<think>") => {
                                        is_reasoning = Some(true);
                                        *content = content.replace("<think>", "");
                                    }
                                    Some(true) if content.contains("</think>") => {
                                        is_reasoning = Some(false);
                                        *content = content.replace("</think>", "");
                                    }
                                    _ => {}
                                }

                                let _ = sender
                                    .send(if is_reasoning.unwrap_or_default() {
                                        Token::Reasoning(content.clone())
                                    } else {
                                        Token::Talking(content.clone())
                                    })
                                    .await;
                            }
                        }
                    };
                }

                buffer = last_line.to_vec();
            }

            Ok(())
        })
    }

    pub fn file(&self) -> &model::File {
        &self.file
    }

    pub fn name(&self) -> &str {
        self.file.model.name()
    }

    fn launch_with_executable(
        executable: &'static str,
        file: &model::File,
        backend: Backend,
    ) -> Result<process::Child, Error> {
        let gpu_flags = match backend {
            Backend::Cpu => "",
            Backend::Cuda | Backend::Rocm => "--gpu-layers 80",
        };

        let server = process::Command::new(executable)
            .args(Self::parse_args(&format!(
                "--model models/{filename} \
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backend {
    Cpu,
    Cuda,
    Rocm,
}

impl Backend {
    pub fn detect(graphics_adapter: &str) -> Self {
        if graphics_adapter.contains("NVIDIA") {
            Self::Cuda
        } else if graphics_adapter.contains("AMD") {
            Self::Rocm
        } else {
            Self::Cpu
        }
    }

    pub fn uses_gpu(self) -> bool {
        match self {
            Backend::Cuda | Backend::Rocm => true,
            Backend::Cpu => false,
        }
    }
}

#[derive(Debug, Clone)]
pub enum Message {
    System(String),
    Assistant(String),
    User(String),
}

impl Message {
    pub fn to_tuple(&self) -> (&'static str, &str) {
        match self {
            Self::System(content) => ("system", content),
            Self::Assistant(content) => ("assistant", content),
            Self::User(content) => ("user", content),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Reply {
    pub reasoning: Option<Reasoning>,
    pub content: String,
}

#[derive(Debug, Clone)]
pub struct Reasoning {
    pub content: String,
    pub duration: Duration,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Token {
    Reasoning(String),
    Talking(String),
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
                    .args(["stop", id])
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
