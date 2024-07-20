pub mod assistant;
pub mod chat;

use std::io;
use std::sync::Arc;

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
