//! Outbound adapter: `PostgresStore` implements the application's store ports with SQLx.
//!
//! Queries are explicit SQL with parameters (REAS-DATA-001). Multi-row writes run in one
//! transaction; lifecycle transitions are conditional updates (REAS-DATA-002). Queries are
//! checked at run time, so compilation never needs a database (REAS-DATA-004).

mod analysis_store;
mod game_store;
mod import_log;
mod job_store;
mod player_store;
pub mod records;
mod report_store;

use std::time::{Duration, Instant};

use sqlx::postgres::{PgPool, PgPoolOptions};
use tracing::{info, warn};

use crate::application::ports::{StoreError, StoreHealth};

/// The one concrete store adapter. Implements `GameLog`, `GameStore`, `ImportLog`,
/// `JobStore`, `AnalysisStore` and `ReportStore`.
#[derive(Debug, Clone)]
pub struct PostgresStore {
    pool: PgPool,
}

impl From<sqlx::Error> for StoreError {
    fn from(e: sqlx::Error) -> Self {
        match e {
            sqlx::Error::PoolTimedOut | sqlx::Error::PoolClosed | sqlx::Error::Io(_) => {
                StoreError::Unavailable(e.to_string())
            }
            sqlx::Error::Decode(_) | sqlx::Error::ColumnDecode { .. } => {
                StoreError::InvalidData(e.to_string())
            }
            other => StoreError::Query(other.to_string()),
        }
    }
}

impl From<sqlx::migrate::MigrateError> for StoreError {
    fn from(e: sqlx::migrate::MigrateError) -> Self {
        StoreError::Query(format!("migration failed: {e}"))
    }
}

impl PostgresStore {
    /// Connect, retrying until `timeout` elapses. Lets the application start while the
    /// database container is still initialising.
    pub async fn connect_with_retry(
        url: &str,
        max_connections: u32,
        timeout: Duration,
    ) -> Result<Self, StoreError> {
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
                    return Ok(Self { pool });
                }
                Err(e) => {
                    if start.elapsed() >= timeout {
                        return Err(StoreError::Unavailable(format!(
                            "could not connect to PostgreSQL after {attempt} attempts ({:?}): {e}",
                            start.elapsed()
                        )));
                    }
                    warn!(attempt, error = %e, retry_in_ms = delay.as_millis() as u64, "PostgreSQL not ready");
                    tokio::time::sleep(delay).await;
                    delay = (delay * 2).min(Duration::from_secs(5));
                }
            }
        }
    }

    /// Wrap an existing pool (integration tests).
    pub fn from_pool(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn migrate(&self) -> Result<(), StoreError> {
        sqlx::migrate!("./migrations").run(&self.pool).await?;
        Ok(())
    }

    pub async fn close(&self) {
        self.pool.close().await;
    }

    pub(crate) async fn health_check(&self) -> StoreHealth {
        let start = Instant::now();
        let res: Result<(String, String, i64), sqlx::Error> = async {
            let version: (String,) = sqlx::query_as("SELECT current_setting('server_version')")
                .fetch_one(&self.pool)
                .await?;
            let size: (String,) =
                sqlx::query_as("SELECT pg_size_pretty(pg_database_size(current_database()))")
                    .fetch_one(&self.pool)
                    .await?;
            let migrations: (i64,) =
                sqlx::query_as("SELECT count(*) FROM _sqlx_migrations WHERE success")
                    .fetch_one(&self.pool)
                    .await?;
            Ok((version.0, size.0, migrations.0))
        }
        .await;
        let latency_ms = start.elapsed().as_secs_f64() * 1000.0;
        let (pool_size, pool_idle) = (self.pool.size(), self.pool.num_idle());
        match res {
            Ok((server_version, database_size, migrations_applied)) => StoreHealth {
                ok: true,
                latency_ms,
                server_version,
                database_size,
                pool_size,
                pool_idle,
                migrations_applied,
                error: None,
            },
            Err(e) => StoreHealth {
                ok: false,
                latency_ms,
                server_version: String::new(),
                database_size: String::new(),
                pool_size,
                pool_idle,
                migrations_applied: 0,
                error: Some(e.to_string()),
            },
        }
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
        let r = PostgresStore::connect_with_retry(
            "postgres://u:p@127.0.0.1:1/none",
            1,
            Duration::from_millis(1200),
        )
        .await;
        assert!(matches!(r, Err(StoreError::Unavailable(_))));
        assert!(start.elapsed() >= Duration::from_millis(1000));
    }
}
