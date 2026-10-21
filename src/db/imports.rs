use crate::models::DataImport;

use super::Pool;

const COLS: &str = "id, source, source_url, file_name, file_sha256, format, status, rows_read, games_inserted, games_updated,
    games_unchanged, rows_rejected, min_season, max_season, error, notes, started_at, finished_at";

pub async fn start(
    pool: &Pool,
    source: &str,
    source_url: Option<&str>,
    file_name: Option<&str>,
    sha256: &str,
    format: &str,
) -> anyhow::Result<i64> {
    let r: (i64,) = sqlx::query_as(
        "INSERT INTO data_imports (source, source_url, file_name, file_sha256, format, status)
         VALUES ($1, $2, $3, $4, $5, 'running') RETURNING id",
    )
    .bind(source)
    .bind(source_url)
    .bind(file_name)
    .bind(sha256)
    .bind(format)
    .fetch_one(pool)
    .await?;
    Ok(r.0)
}

pub struct ImportStats {
    pub rows_read: i64,
    pub games_inserted: i64,
    pub games_updated: i64,
    pub games_unchanged: i64,
    pub rows_rejected: i64,
    pub min_season: Option<i32>,
    pub max_season: Option<i32>,
    pub notes: Option<String>,
}

pub async fn finish(pool: &Pool, id: i64, stats: &ImportStats) -> anyhow::Result<()> {
    sqlx::query(
        "UPDATE data_imports SET status = 'completed', rows_read = $2, games_inserted = $3, games_updated = $4,
                games_unchanged = $5, rows_rejected = $6, min_season = $7, max_season = $8, notes = $9, finished_at = now()
         WHERE id = $1",
    )
    .bind(id)
    .bind(stats.rows_read)
    .bind(stats.games_inserted)
    .bind(stats.games_updated)
    .bind(stats.games_unchanged)
    .bind(stats.rows_rejected)
    .bind(stats.min_season)
    .bind(stats.max_season)
    .bind(&stats.notes)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn fail(pool: &Pool, id: i64, error: &str) -> anyhow::Result<()> {
    sqlx::query("UPDATE data_imports SET status = 'failed', error = $2, finished_at = now() WHERE id = $1")
        .bind(id)
        .bind(error)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn list(pool: &Pool, limit: i64) -> anyhow::Result<Vec<DataImport>> {
    Ok(sqlx::query_as::<_, DataImport>(&format!("SELECT {COLS} FROM data_imports ORDER BY started_at DESC LIMIT $1"))
        .bind(limit)
        .fetch_all(pool)
        .await?)
}

pub async fn completed_with_sha(pool: &Pool, sha256: &str) -> anyhow::Result<Option<DataImport>> {
    Ok(sqlx::query_as::<_, DataImport>(&format!(
        "SELECT {COLS} FROM data_imports WHERE file_sha256 = $1 AND status = 'completed' ORDER BY started_at DESC LIMIT 1"
    ))
    .bind(sha256)
    .fetch_optional(pool)
    .await?)
}

/// Imports left 'running' by a crashed process are marked failed on startup.
pub async fn mark_stale_failed(pool: &Pool) -> anyhow::Result<u64> {
    let r = sqlx::query(
        "UPDATE data_imports SET status = 'failed', error = 'interrupted by application restart', finished_at = now()
         WHERE status = 'running'",
    )
    .execute(pool)
    .await?;
    Ok(r.rows_affected())
}
