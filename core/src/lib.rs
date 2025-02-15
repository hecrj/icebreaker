pub mod assistant;
pub mod chat;
pub mod model;
pub mod plan;
pub mod web;

pub use assistant::Assistant;
pub use chat::Chat;
pub use model::Model;
pub use plan::Plan;
pub use url::Url;

mod request;

use std::io;
use std::sync::Arc;
use tokio::task;

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
    #[error("deserialization failed: {0}")]
    DecodingFailed(Arc<serde_json::Error>),
    #[error("task join failed: {0}")]
    JoinFailed(Arc<task::JoinError>),
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

impl From<serde_json::Error> for Error {
    fn from(error: serde_json::Error) -> Self {
        Self::DecodingFailed(Arc::new(error))
    }
}

impl From<task::JoinError> for Error {
    fn from(error: task::JoinError) -> Self {
        Self::JoinFailed(Arc::new(error))
    }
}
