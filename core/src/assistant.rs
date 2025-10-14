use crate::model;
use crate::Error;

use serde::Deserialize;
use serde_json::json;
use sipper::{sipper, FutureExt, Sipper, Straw, StreamExt};

use std::process::Stdio;
use std::sync::Arc;
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub struct Assistant {
    file: model::File,
    _server: Arc<Server>,
}

impl Assistant {
    const HOST_PORT: u32 = 8080;

    pub fn boot(
        directory: model::Directory,
        file: model::File,
        backend: Backend,
    ) -> impl Straw<Self, BootEvent, Error> {
        use tokio::io::{self, AsyncBufReadExt};
        use tokio::task;

        #[derive(Clone)]
        struct Sender(sipper::Sender<BootEvent>);

        impl Sender {
            async fn log(&mut self, log: String) {
                let _ = self.0.send(BootEvent::Logged(log)).await;
            }

            async fn log_download(
                &mut self,
                downloaded: u64,
                total: u64,
                speed: u64,
                percent: u32,
            ) {
                self.log(format!(
                    "=> {percent}% {downloaded:.2}GB of {total:.2}GB \
                                    @ {speed:.2} MB/s",
                    downloaded = downloaded as f32 / 10f32.powi(9),
                    total = total as f32 / 10f32.powi(9),
                    speed = speed as f32 / 10f32.powi(6),
                ))
                .await;
            }

            async fn progress(&mut self, stage: impl Into<String>, percent: u32) {
                let _ = self
                    .0
                    .send(BootEvent::Progressed {
                        stage: stage.into(),
                        percent,
                    })
                    .await;
            }
        }

        sipper(move |sender| async move {
            let mut sender = Sender(sender);

            let builds = llama_server::Server::list().await?;

            let build = if let Some(latest) = builds.last() {
                *latest
            } else {
                llama_server::Build::latest()
                    .await
                    .ok()
                    .unwrap_or(llama_server::Build::locked(6756))
            };

            let mut server = llama_server::Server::download(
                build,
                match backend {
                    Backend::Cpu => llama_server::backend::Set::empty(),
                    Backend::Cuda => llama_server::backend::Set::CUDA,
                    Backend::Rocm => llama_server::backend::Set::HIP,
                },
            )
            .pin();

            let mut last_percent = None;

            while let Some(stage) = server.sip().await {
                match stage {
                    llama_server::Stage::Downloading(artifact, progress) => {
                        let percent =
                            ((progress.downloaded as f32 / progress.total as f32) * 100.0) as u32;

                        if last_percent == Some(percent) {
                            continue;
                        }

                        let component = match artifact {
                            llama_server::Artifact::Server => "llama-server",
                            llama_server::Artifact::Backend(backend) => match backend {
                                llama_server::Backend::Cuda => "CUDA backend",
                                llama_server::Backend::Hip => "ROCm backend",
                            },
                        };

                        sender
                            .progress(format!("Downloading {component}..."), percent)
                            .await;

                        sender
                            .log_download(
                                progress.downloaded,
                                progress.total,
                                progress.speed,
                                percent,
                            )
                            .await;

                        last_percent = Some(percent);
                    }
                }
            }

            let server = server.await?;
            let mut model = file.download(&directory).pin();

            while let Some(progress) = model.sip().await {
                if let Some((total, percent)) = progress.percent() {
                    if last_percent == Some(percent) {
                        continue;
                    }

                    sender.progress("Downloading model...", percent).await;

                    sender
                        .log_download(progress.downloaded, total, progress.speed, percent)
                        .await;

                    last_percent = Some(percent);
                }
            }

            let model_path = model.await?;

            sender.progress("Loading model...", 99).await;

            let mut instance = server
                .boot(
                    model_path,
                    llama_server::Settings {
                        host: String::from("127.0.0.1"),
                        port: Self::HOST_PORT,
                        gpu_layers: 80,
                        stdin: Stdio::null(),
                        stdout: Stdio::piped(),
                        stderr: Stdio::piped(),
                    },
                )
                .await?;

            let stdout = instance.process.stdout.take();
            let stderr = instance.process.stderr.take();

            let log_output = {
                let mut sender = sender.clone();

                let mut lines = {
                    use futures::stream;
                    use tokio_stream::wrappers::LinesStream;

                    let stdout = stdout.expect("piped stdout");
                    let stderr = stderr.expect("piped stderr");

                    let stdout = io::BufReader::new(stdout);
                    let stderr = io::BufReader::new(stderr);

                    stream::select(
                        LinesStream::new(stdout.lines()),
                        LinesStream::new(stderr.lines()),
                    )
                };

                async move {
                    while let Some(line) = lines.next().await {
                        if let Ok(log) = line {
                            log::debug!("{log}");
                            sender.log(log).await;
                        }
                    }

                    false
                }
                .boxed()
            };

            let log_handle = task::spawn(log_output);
            instance.wait_until_ready().await?;
            log_handle.abort();

            Ok(Self {
                file,
                _server: Arc::new(Server {
                    _instance: instance,
                }),
            })
        })
    }

    pub fn reply<'a>(
        &'a self,
        prompt: &'a str,
        messages: &'a [Message],
        append: &'a [Message],
    ) -> impl Straw<Reply, (Reply, Token), Error> + 'a {
        sipper(move |mut progress| async move {
            let mut reasoning = None;
            let mut reasoning_started_at: Option<Instant> = None;
            let mut content = String::new();
            let mut reasoning_content = String::new();

            let mut completion = self.complete(prompt, messages, append).pin();

            while let Some(token) = completion.sip().await {
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

                progress
                    .send((
                        Reply {
                            reasoning: reasoning.clone(),
                            content: content.trim().to_owned(),
                            last_token: if let Token::Talking(token) = &token {
                                Some(token.clone())
                            } else {
                                None
                            },
                        },
                        token,
                    ))
                    .await;
            }

            Ok(Reply {
                reasoning: reasoning.clone(),
                content: content.trim().to_owned(),
                last_token: None,
            })
        })
    }

    pub fn complete<'a>(
        &'a self,
        system_prompt: &'a str,
        messages: &'a [Message],
        append: &'a [Message],
    ) -> impl Straw<(), Token, Error> + 'a {
        sipper(move |mut sender| async move {
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
    pub last_token: Option<String>,
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
struct Server {
    _instance: llama_server::Instance,
}

#[derive(Debug, Clone)]
pub enum BootEvent {
    Progressed { stage: String, percent: u32 },
    Logged(String),
}
