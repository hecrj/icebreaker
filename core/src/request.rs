use crate::Error;

use reqwest::IntoUrl;
use sipper::{sipper, Straw};
use tokio::fs;
use tokio::io::{self, AsyncWriteExt};

use std::path::Path;
use std::time::Instant;

#[derive(Debug, Clone, Copy)]
pub struct Progress {
    pub total: Option<u64>,
    pub downloaded: u64,
    pub speed: u64,
}

impl Progress {
    pub fn percent(self) -> Option<(u64, u32)> {
        let total = self.total?;

        Some((
            total,
            (self.downloaded as f32 / total as f32 * 100.0).round() as u32,
        ))
    }
}

pub fn download_file<'a>(
    url: impl IntoUrl + Send + 'a,
    destination: impl AsRef<Path> + Send + 'a,
) -> impl Straw<(), Progress, Error> + 'a {
    sipper(move |mut progress| async move {
        let destination = destination.as_ref();
        let mut file = io::BufWriter::new(fs::File::create(destination).await?);

        let mut download = reqwest::get(url).await?;
        let start = Instant::now();
        let total = download.content_length();
        let mut downloaded = 0;

        progress
            .send(Progress {
                total,
                downloaded,
                speed: 0,
            })
            .await;

        while let Some(chunk) = download.chunk().await? {
            downloaded += chunk.len() as u64;
            let speed = (downloaded as f32 / start.elapsed().as_secs_f32()) as u64;

            progress
                .send(Progress {
                    total,
                    downloaded,
                    speed,
                })
                .await;

            file.write_all(&chunk).await?;
        }

        file.flush().await?;

        Ok(())
    })
}
