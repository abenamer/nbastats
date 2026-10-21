//! Outbound adapter: download the source dataset over HTTPS.

use std::time::Duration;

use crate::application::ports::{DatasetSource, DownloadError};

#[derive(Debug, Clone)]
pub struct HttpDatasetSource {
    client: reqwest::Client,
}

impl HttpDatasetSource {
    pub fn new() -> Result<Self, DownloadError> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(300))
            .user_agent(concat!("nba-elo-simulator/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|e| DownloadError::Transport(e.to_string()))?;
        Ok(Self { client })
    }
}

impl DatasetSource for HttpDatasetSource {
    async fn download(&self, url: &str) -> Result<Vec<u8>, DownloadError> {
        let resp = self
            .client
            .get(url)
            .send()
            .await
            .and_then(reqwest::Response::error_for_status)
            .map_err(|e| DownloadError::Transport(e.to_string()))?;
        let bytes = resp
            .bytes()
            .await
            .map_err(|e| DownloadError::Transport(e.to_string()))?;
        if bytes.len() < 100 {
            return Err(DownloadError::TooSmall(bytes.len()));
        }
        Ok(bytes.to_vec())
    }
}
