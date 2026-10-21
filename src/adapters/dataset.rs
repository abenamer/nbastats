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

/// Upper bound on a downloaded dataset (the full game log is about 16 MB).
pub const MAX_DOWNLOAD_BYTES: usize = 256 * 1024 * 1024;

impl DatasetSource for HttpDatasetSource {
    async fn download(&self, url: &str) -> Result<Vec<u8>, DownloadError> {
        let mut resp = self
            .client
            .get(url)
            .send()
            .await
            .and_then(reqwest::Response::error_for_status)
            .map_err(|e| DownloadError::Transport(e.to_string()))?;
        if let Some(len) = resp.content_length() {
            if len as usize > MAX_DOWNLOAD_BYTES {
                return Err(DownloadError::TooLarge(len as usize));
            }
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = resp
            .chunk()
            .await
            .map_err(|e| DownloadError::Transport(e.to_string()))?
        {
            if bytes.len() + chunk.len() > MAX_DOWNLOAD_BYTES {
                return Err(DownloadError::TooLarge(bytes.len() + chunk.len()));
            }
            bytes.extend_from_slice(&chunk);
        }
        if bytes.len() < 100 {
            return Err(DownloadError::TooSmall(bytes.len()));
        }
        Ok(bytes)
    }
}
