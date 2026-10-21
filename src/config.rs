//! Environment-based configuration. Every value has a sane default for local use; secrets
//! (the database URL) must come from the environment or a `.env` file.

use std::env;
use std::path::PathBuf;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct Config {
    pub database_url: String,
    pub bind_addr: String,
    pub data_dir: PathBuf,
    pub db_max_connections: u32,
    pub db_connect_timeout: Duration,
    pub auto_import_bundled: bool,
    pub bundled_dataset: PathBuf,
    pub data_source_url: String,
    pub max_iterations: u64,
    pub worker_threads: Option<usize>,
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        let database_url = env::var("DATABASE_URL")
            .map_err(|_| anyhow::anyhow!("DATABASE_URL must be set (see .env.example)"))?;
        let bind_addr = env::var("APP_BIND").unwrap_or_else(|_| "0.0.0.0:3000".to_string());
        let data_dir = PathBuf::from(env::var("APP_DATA_DIR").unwrap_or_else(|_| "./app-data".to_string()));
        let db_max_connections = env::var("DB_MAX_CONNECTIONS").ok().and_then(|v| v.parse().ok()).unwrap_or(8);
        let db_connect_timeout = Duration::from_secs(
            env::var("DB_CONNECT_TIMEOUT_SECS").ok().and_then(|v| v.parse().ok()).unwrap_or(120),
        );
        let auto_import_bundled = env::var("AUTO_IMPORT_BUNDLED")
            .map(|v| matches!(v.to_ascii_lowercase().as_str(), "1" | "true" | "yes"))
            .unwrap_or(true);
        let bundled_dataset = PathBuf::from(
            env::var("BUNDLED_DATASET").unwrap_or_else(|_| "./data/nba_elo_neil_paine.csv.gz".to_string()),
        );
        let data_source_url = env::var("DATA_SOURCE_URL").unwrap_or_else(|_| {
            "https://raw.githubusercontent.com/Neil-Paine-1/NBA-elo/master/nba_elo.csv".to_string()
        });
        let max_iterations = env::var("MAX_ITERATIONS").ok().and_then(|v| v.parse().ok()).unwrap_or(5_000_000);
        let worker_threads = env::var("SIM_THREADS").ok().and_then(|v| v.parse().ok());
        Ok(Self {
            database_url,
            bind_addr,
            data_dir,
            db_max_connections,
            db_connect_timeout,
            auto_import_bundled,
            bundled_dataset,
            data_source_url,
            max_iterations,
            worker_threads,
        })
    }

    /// Database URL with the password masked, safe for logs and the UI.
    pub fn masked_database_url(&self) -> String {
        mask_url(&self.database_url)
    }
}

pub fn mask_url(url: &str) -> String {
    if let Some(scheme_end) = url.find("://") {
        let rest = &url[scheme_end + 3..];
        if let Some(at) = rest.rfind('@') {
            let creds = &rest[..at];
            let user = creds.split(':').next().unwrap_or("");
            return format!("{}://{}:****@{}", &url[..scheme_end], user, &rest[at + 1..]);
        }
    }
    url.to_string()
}

pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn masks_password() {
        assert_eq!(mask_url("postgres://nba:secret@db:5432/nba"), "postgres://nba:****@db:5432/nba");
        assert_eq!(mask_url("postgres://db/nba"), "postgres://db/nba");
    }
}
