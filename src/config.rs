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
    /// Upper bound on the estimated elementary operations of one job (draws × per-draw work).
    pub max_work_units: u64,
    pub worker_threads: Option<usize>,
    /// Host names accepted in the `Host` header; `*` accepts any.
    pub allowed_hosts: Vec<String>,
    /// BALLDONTLIE API key; when unset, the results sync is unavailable.
    pub balldontlie_api_key: Option<String>,
    pub balldontlie_base_url: String,
    /// Request pacing for the BALLDONTLIE quota (the free tier allows 5 per minute).
    pub balldontlie_requests_per_minute: u32,
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        let database_url = env::var("DATABASE_URL")
            .map_err(|_| anyhow::anyhow!("DATABASE_URL must be set (see .env.example)"))?;
        let bind_addr = env::var("APP_BIND").unwrap_or_else(|_| "0.0.0.0:3000".to_string());
        let data_dir =
            PathBuf::from(env::var("APP_DATA_DIR").unwrap_or_else(|_| "./app-data".to_string()));
        let db_max_connections = env::var("DB_MAX_CONNECTIONS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(8);
        let db_connect_timeout = Duration::from_secs(
            env::var("DB_CONNECT_TIMEOUT_SECS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(120),
        );
        let auto_import_bundled = env::var("AUTO_IMPORT_BUNDLED")
            .map(|v| matches!(v.to_ascii_lowercase().as_str(), "1" | "true" | "yes"))
            .unwrap_or(true);
        let bundled_dataset = PathBuf::from(
            env::var("BUNDLED_DATASET")
                .unwrap_or_else(|_| "./data/nba_elo_neil_paine.csv.gz".to_string()),
        );
        let data_source_url = env::var("DATA_SOURCE_URL").unwrap_or_else(|_| {
            "https://raw.githubusercontent.com/Neil-Paine-1/NBA-elo/master/nba_elo.csv".to_string()
        });
        let max_iterations = env::var("MAX_ITERATIONS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(5_000_000);
        let max_work_units = env::var("MAX_WORK_UNITS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(4_000_000_000);
        let worker_threads = env::var("SIM_THREADS").ok().and_then(|v| v.parse().ok());
        let allowed_hosts = env::var("ALLOWED_HOSTS")
            .unwrap_or_else(|_| "localhost,127.0.0.1,[::1]".to_string())
            .split(',')
            .map(|h| h.trim().to_ascii_lowercase())
            .filter(|h| !h.is_empty())
            .collect();
        let balldontlie_api_key = env::var("BALLDONTLIE_API_KEY")
            .ok()
            .map(|k| k.trim().to_string())
            .filter(|k| !k.is_empty());
        let balldontlie_base_url = env::var("BALLDONTLIE_BASE_URL")
            .ok()
            .filter(|u| !u.trim().is_empty())
            .unwrap_or_else(|| "https://api.balldontlie.io/v1".to_string());
        let balldontlie_requests_per_minute = env::var("BALLDONTLIE_REQUESTS_PER_MINUTE")
            .ok()
            .and_then(|v| v.parse().ok())
            .filter(|&n: &u32| n > 0)
            .unwrap_or(5);
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
            max_work_units,
            worker_threads,
            allowed_hosts,
            balldontlie_api_key,
            balldontlie_base_url,
            balldontlie_requests_per_minute,
        })
    }

    /// Whether a request `Host` header (with or without port) is accepted.
    pub fn host_allowed(&self, host: &str) -> bool {
        if self.allowed_hosts.iter().any(|h| h == "*") {
            return true;
        }
        let host = host.trim().to_ascii_lowercase();
        // Strip a port: "localhost:3000", "[::1]:3000".
        let bare = if let Some(end) = host.strip_prefix('[').and_then(|_| host.find(']')) {
            host[..=end].to_string()
        } else {
            host.rsplit_once(':')
                .map_or(host.clone(), |(h, _)| h.to_string())
        };
        self.allowed_hosts.contains(&bare)
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

    fn cfg(hosts: &str) -> Config {
        Config {
            database_url: String::new(),
            bind_addr: String::new(),
            data_dir: PathBuf::new(),
            db_max_connections: 1,
            db_connect_timeout: Duration::from_secs(1),
            auto_import_bundled: false,
            bundled_dataset: PathBuf::new(),
            data_source_url: String::new(),
            max_iterations: 1,
            max_work_units: 1,
            worker_threads: None,
            allowed_hosts: hosts.split(',').map(str::to_string).collect(),
            balldontlie_api_key: None,
            balldontlie_base_url: String::new(),
            balldontlie_requests_per_minute: 5,
        }
    }

    #[test]
    fn host_allowlist() {
        let c = cfg("localhost,127.0.0.1,[::1]");
        assert!(c.host_allowed("localhost:3000"));
        assert!(c.host_allowed("LOCALHOST"));
        assert!(c.host_allowed("127.0.0.1:3100"));
        assert!(c.host_allowed("[::1]:3000"));
        assert!(!c.host_allowed("evil.example:3000"));
        assert!(!c.host_allowed("localhost.evil.example"));
        assert!(cfg("*").host_allowed("anything"));
        assert!(cfg("nas.local").host_allowed("nas.local:3000"));
    }

    #[test]
    fn masks_password() {
        assert_eq!(
            mask_url("postgres://nba:secret@db:5432/nba"),
            "postgres://nba:****@db:5432/nba"
        );
        assert_eq!(mask_url("postgres://db/nba"), "postgres://db/nba");
    }
}
