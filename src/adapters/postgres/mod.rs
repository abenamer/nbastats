//! PostgreSQL access: connection pool with startup retry, migrations, health checks and
//! query modules.

pub mod analysis;
pub mod games;
pub mod imports;
pub mod jobs;
pub mod teams;

use std::time::{Duration, Instant};

use sqlx::postgres::{PgPool, PgPoolOptions};
use tracing::{info, warn};

pub type Pool = PgPool;

/// Connect to PostgreSQL, retrying until `timeout` elapses. This lets the application start
/// while the database container is still initialising (the Compose health check covers the
/// common case; this covers restarts and manual runs).
pub async fn connect_with_retry(url: &str, max_connections: u32, timeout: Duration) -> anyhow::Result<Pool> {
    let start = Instant::now();
    let mut delay = Duration::from_millis(500);
    let mut attempt = 0u32;
    loop {
        attempt += 1;
        match PgPoolOptions::new()
            .max_connections(max_connections)
            .acquire_timeout(Duration::from_secs(10))
            .connect(url)
            .await
        {
            Ok(pool) => {
                info!(attempt, "connected to PostgreSQL");
                return Ok(pool);
            }
            Err(e) => {
                if start.elapsed() >= timeout {
                    return Err(anyhow::anyhow!(
                        "could not connect to PostgreSQL after {attempt} attempts ({:?}): {e}",
                        start.elapsed()
                    ));
                }
                warn!(attempt, error = %e, retry_in_ms = delay.as_millis() as u64, "PostgreSQL not ready");
                tokio::time::sleep(delay).await;
                delay = (delay * 2).min(Duration::from_secs(5));
            }
        }
    }
}

pub async fn migrate(pool: &Pool) -> anyhow::Result<()> {
    sqlx::migrate!("./migrations").run(pool).await?;
    Ok(())
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct DbHealth {
    pub ok: bool,
    pub latency_ms: f64,
    pub server_version: String,
    pub database_size: String,
    pub pool_size: u32,
    pub pool_idle: usize,
    pub migrations_applied: i64,
    pub error: Option<String>,
}

pub async fn health(pool: &Pool) -> DbHealth {
    let start = Instant::now();
    let res: Result<(String, String, i64), sqlx::Error> = async {
        let version: (String,) = sqlx::query_as("SELECT current_setting('server_version')").fetch_one(pool).await?;
        let size: (String,) = sqlx::query_as("SELECT pg_size_pretty(pg_database_size(current_database()))")
            .fetch_one(pool)
            .await?;
        let migrations: (i64,) = sqlx::query_as("SELECT count(*) FROM _sqlx_migrations WHERE success")
            .fetch_one(pool)
            .await?;
        Ok((version.0, size.0, migrations.0))
    }
    .await;
    let latency_ms = start.elapsed().as_secs_f64() * 1000.0;
    match res {
        Ok((v, s, m)) => DbHealth {
            ok: true,
            latency_ms,
            server_version: v,
            database_size: s,
            pool_size: pool.size(),
            pool_idle: pool.num_idle(),
            migrations_applied: m,
            error: None,
        },
        Err(e) => DbHealth {
            ok: false,
            latency_ms,
            server_version: String::new(),
            database_size: String::new(),
            pool_size: pool.size(),
            pool_idle: pool.num_idle(),
            migrations_applied: 0,
            error: Some(e.to_string()),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn connect_retry_gives_up_after_timeout() {
        // Nothing listens on this port; the connect loop must keep retrying and then fail
        // cleanly with a descriptive error instead of panicking.
        let start = Instant::now();
        let r = connect_with_retry("postgres://u:p@127.0.0.1:1/none", 1, Duration::from_millis(1200)).await;
        assert!(r.is_err());
        assert!(start.elapsed() >= Duration::from_millis(1000));
        assert!(r.unwrap_err().to_string().contains("attempts"));
    }
}
