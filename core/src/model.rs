use crate::request;
use crate::Error;

use serde::{Deserialize, Serialize};
use sipper::{sipper, Sipper, Straw};
use tokio::fs;

use std::collections::BTreeMap;
use std::fmt;
use std::path::PathBuf;

const HF_URL: &str = "https://huggingface.co";
const API_URL: &str = "https://huggingface.co/api";

#[derive(Debug, Clone)]
pub struct Model {
    pub id: Id,
    pub last_modified: chrono::DateTime<chrono::Local>,
    pub downloads: Downloads,
    pub likes: Likes,
}

impl Model {
    pub async fn list() -> Result<Vec<Self>, Error> {
        Self::search(String::new()).await
    }

    pub async fn search(query: String) -> Result<Vec<Self>, Error> {
        let client = reqwest::Client::new();

        let request = client.get(format!("{API_URL}/models")).query(&[
            ("search", query.as_ref()),
            ("filter", "text-generation"),
            ("filter", "gguf"),
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
        }

        #[derive(Deserialize, PartialEq, Eq)]
        #[serde(untagged)]
        enum Gated {
            Bool(bool),
            Other(String),
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
            })
            .collect())
    }
}

impl fmt::Display for Model {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.id.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Id(pub(crate) String);

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

#[derive(Debug, Clone)]
pub struct Details {
    pub last_modified: chrono::DateTime<chrono::Local>,
    pub downloads: Downloads,
    pub likes: Likes,
    pub architecture: Option<String>,
    pub parameters: Parameters,
}

impl Details {
    pub async fn fetch(id: Id) -> Result<Self, Error> {
        #[derive(Deserialize)]
        struct Response {
            #[serde(rename = "lastModified")]
            last_modified: chrono::DateTime<chrono::Local>,
            downloads: Downloads,
            likes: Likes,
            gguf: Gguf,
        }

        #[derive(Deserialize)]
        struct Gguf {
            #[serde(default)]
            architecture: Option<String>,
            total: u64,
        }

        let client = reqwest::Client::new();
        let request = client.get(format!("{}/models/{}", API_URL, id.0));

        let response: Response = request.send().await?.error_for_status()?.json().await?;

        Ok(Self {
            last_modified: response.last_modified,
            downloads: response.downloads,
            likes: response.likes,
            architecture: response.gguf.architecture,
            parameters: Parameters(response.gguf.total),
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub struct Downloads(u64);

impl fmt::Display for Downloads {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            1_000_000.. => {
                write!(f, "{:.2}M", (self.0 as f32 / 1_000_000_f32))
            }
            1_000.. => {
                write!(f, "{:.2}k", (self.0 as f32 / 1_000_f32))
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Deserialize)]
pub struct Parameters(u64);

impl fmt::Display for Parameters {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0.ilog10() {
            0..3 => write!(f, "{}", self.0),
            3..6 => write!(f, "{}K", self.0 / 1000),
            6..9 => write!(f, "{}M", self.0 / 1_000_000),
            9..12 => write!(f, "{}B", self.0 / 1_000_000_000),
            _ => write!(f, "{}T", self.0 / 1_000_000_000),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct File {
    pub model: Id,
    pub name: String,
    #[serde(default)]
    pub size: Option<Size>,
}

impl File {
    pub async fn list(id: Id) -> Result<Files, Error> {
        let client = reqwest::Client::new();
        let request = client.get(format!("{}/models/{}/tree/main", API_URL, id.0));

        #[derive(Debug, Deserialize)]
        struct Entry {
            r#type: String,
            path: String,
            size: u64,
        }

        let entries: Vec<Entry> = request.send().await?.error_for_status()?.json().await?;
        let mut files: BTreeMap<Bits, Vec<File>> = BTreeMap::new();

        for entry in entries {
            if entry.r#type != "file" || !entry.path.ends_with(".gguf") {
                continue;
            }

            let file_stem = entry.path.trim_end_matches(".gguf");
            let variant = file_stem.rsplit(['-', '.']).next().unwrap_or(file_stem);
            let precision = variant
                .split('_')
                .next()
                .unwrap_or(variant)
                .trim_start_matches("IQ")
                .trim_start_matches("Q")
                .trim_start_matches("BF")
                .trim_start_matches("F")
                .parse()
                .map(Bits);

            let Ok(precision) = precision else {
                continue;
            };

            let files = files.entry(precision).or_default();

            files.push(File {
                model: id.clone(),
                name: entry.path,
                size: Some(Size(entry.size)),
            })
        }

        Ok(files)
    }

    pub fn decode(value: decoder::Value) -> decoder::Result<Self> {
        use decoder::decode::{map, string, u64};

        let mut file = map(value)?;

        Ok(Self {
            model: Id(file.required("model", string)?),
            name: file.required("name", string)?,
            size: file.optional("size", u64)?.map(Size),
        })
    }

    pub fn encode(self) -> decoder::Value {
        use decoder::encode::{map, string};

        map([("model", string(self.model.0)), ("name", string(self.name))]).into()
    }

    pub fn variant(&self) -> Option<&str> {
        self.name
            .trim_end_matches(".gguf")
            .rsplit(['-', '.'])
            .next()
    }

    pub fn relative_path(&self) -> PathBuf {
        PathBuf::from(&self.model.0).join(&self.name)
    }
}

impl fmt::Display for File {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.name)
    }
}

pub type Files = BTreeMap<Bits, Vec<File>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Bits(u64);

impl fmt::Display for Bits {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}-bit", self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Size(u64);

impl fmt::Display for Size {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0.ilog10() {
            0..3 => write!(f, "{} B", self.0),
            3..6 => write!(f, "{} KB", self.0 / 1000),
            6..9 => write!(f, "{} MB", self.0 / 1_000_000),
            9..12 => write!(f, "{} GB", self.0 / 1_000_000_000),
            _ => write!(f, "{} TB", self.0 / 1_000_000_000_000),
        }
    }
}

impl PartialEq<u64> for Size {
    fn eq(&self, other: &u64) -> bool {
        &self.0 == other
    }
}

#[derive(Debug, Clone)]
pub struct Readme {
    pub markdown: String,
}

impl Readme {
    pub async fn fetch(id: Id) -> Result<Self, Error> {
        let response = reqwest::get(format!(
            "{url}/{id}/raw/main/README.md",
            url = HF_URL,
            id = id.0
        ))
        .await?;

        Ok(Self {
            markdown: response.text().await?,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Library {
    files: Vec<File>,
}

impl Library {
    pub async fn scan() -> Result<Self, Error> {
        let mut files = Vec::new();

        let path = Self::path().await;
        let mut directory = fs::read_dir(path).await?;

        while let Some(author) = directory.next_entry().await? {
            if !author.file_type().await?.is_dir() {
                continue;
            }

            let mut directory = fs::read_dir(author.path()).await?;

            while let Some(model) = directory.next_entry().await? {
                if !model.file_type().await?.is_dir() {
                    continue;
                }

                let mut directory = fs::read_dir(model.path()).await?;

                while let Some(file) = directory.next_entry().await? {
                    if !file.file_type().await?.is_file()
                        || file.path().extension().unwrap_or_default() != "gguf"
                    {
                        continue;
                    }

                    files.push(File {
                        model: Id(format!(
                            "{}/{}",
                            author.file_name().display(),
                            model.file_name().display(),
                        )),
                        name: file.file_name().display().to_string(),
                        size: Some(Size(file.metadata().await?.len())),
                    });
                }
            }
        }

        Ok(Self { files })
    }

    pub fn files(&self) -> &[File] {
        &self.files
    }

    pub async fn path() -> PathBuf {
        // TODO: Configurable
        PathBuf::from("./models")
    }

    pub fn download(file: File) -> impl Straw<PathBuf, request::Progress, Error> {
        sipper(async move |sender| {
            let library = Self::path().await;
            let old_path = library.join(&file.name);
            let directory = library.join(&file.model.0);
            let model_path = directory.join(&file.name);

            fs::create_dir_all(&directory).await?;

            if fs::try_exists(&model_path).await? {
                let file_metadata = fs::metadata(&model_path).await?;

                if file.size.is_none_or(|size| size == file_metadata.len()) {
                    return Ok(model_path);
                }

                fs::remove_file(&model_path).await?;
            }

            if fs::copy(&old_path, &model_path).await.is_ok() {
                let _ = fs::remove_file(old_path).await;
                return Ok(model_path);
            }

            let url = format!(
                "{}/{id}/resolve/main/{filename}?download=true",
                HF_URL,
                id = file.model.0,
                filename = file.name
            );

            let temp_path = model_path.with_extension("tmp");

            request::download_file(url, &temp_path).run(sender).await?;
            fs::rename(temp_path, &model_path).await?;

            Ok(model_path)
        })
    }
}
