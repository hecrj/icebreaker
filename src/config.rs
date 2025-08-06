use std::{path::PathBuf, sync::LazyLock};

static CONFIG_DIR: LazyLock<PathBuf> = LazyLock::new(|| {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("icebreaker")
});

#[derive(Debug, Clone, thiserror::Error)]
pub enum Error {
    #[error("I/O error {0}")]
    Io(std::sync::Arc<std::io::Error>),
    #[error("RON spanned error {0}")]
    RonSpannedError(#[from] ron::error::SpannedError),
    #[error("RON error {0}")]
    RonError(#[from] ron::error::Error),
    #[error("Path not found")]
    PathNotFound,
}

impl From<std::io::Error> for Error {
    fn from(error: std::io::Error) -> Self {
        Self::Io(std::sync::Arc::new(error))
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Config {
    #[serde(default = "default_model_dir")]
    pub model_dir: PathBuf,
}

impl Config {
    pub async fn load() -> Result<Self, Error> {
        let path = CONFIG_DIR.join("config.ron");
        if path.exists() {
            let file = tokio::fs::read_to_string(&path).await?;
            let config: Config = ron::de::from_str(&file)?;
            Ok(config)
        } else {
            Err(Error::PathNotFound)
        }
    }

    pub async fn save(&self) -> Result<(), Error> {
        let path = CONFIG_DIR.join("config.ron");
        let serialized = ron::ser::to_string_pretty(self, ron::ser::PrettyConfig::default())?;
        tokio::fs::create_dir_all(CONFIG_DIR.parent().expect("Failed to get parent directory"))
            .await?;
        tokio::fs::write(path, serialized).await?;
        Ok(())
    }
}

fn default_model_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("icebreaker")
        .join("models")
}

impl Default for Config {
    fn default() -> Self {
        Config {
            model_dir: default_model_dir(),
        }
    }
}
