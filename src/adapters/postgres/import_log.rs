//! `ImportLog` for PostgreSQL.

use crate::application::ports::{ImportLog, ImportStart, ImportStats, StoreResult};
use crate::domain::models::DataImport;

use super::PostgresStore;

const COLS: &str = "id, source, source_url, file_name, file_sha256, format, status, rows_read, games_inserted, games_updated,
    games_unchanged, rows_rejected, min_season, max_season, error, notes, started_at, finished_at";

impl ImportLog for PostgresStore {
    async fn start_import(&self, s: &ImportStart<'_>) -> StoreResult<i64> {
        let r: (i64,) = sqlx::query_as(
            "INSERT INTO data_imports (source, source_url, file_name, file_sha256, format, status)
             VALUES ($1, $2, $3, $4, $5, 'running') RETURNING id",
        )
        .bind(s.source)
        .bind(s.source_url)
        .bind(s.file_name)
        .bind(s.sha256)
        .bind(s.format)
        .fetch_one(&self.pool)
        .await?;
        Ok(r.0)
    }

    async fn finish_import(&self, id: i64, stats: &ImportStats) -> StoreResult<()> {
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
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn fail_import(&self, id: i64, error: &str) -> StoreResult<()> {
        sqlx::query("UPDATE data_imports SET status = 'failed', error = $2, finished_at = now() WHERE id = $1")
            .bind(id)
            .bind(error)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn imports(&self, limit: i64) -> StoreResult<Vec<DataImport>> {
        Ok(sqlx::query_as::<_, DataImport>(&format!(
            "SELECT {COLS} FROM data_imports ORDER BY started_at DESC LIMIT $1"
        ))
        .bind(limit)
        .fetch_all(&self.pool)
        .await?)
    }

    async fn completed_import_with_sha(&self, sha256: &str) -> StoreResult<Option<DataImport>> {
        Ok(sqlx::query_as::<_, DataImport>(&format!(
            "SELECT {COLS} FROM data_imports WHERE file_sha256 = $1 AND status = 'completed' ORDER BY started_at DESC LIMIT 1"
        ))
        .bind(sha256)
        .fetch_optional(&self.pool)
        .await?)
    }

    async fn mark_stale_imports_failed(&self) -> StoreResult<u64> {
        let r = sqlx::query(
            "UPDATE data_imports SET status = 'failed', error = 'interrupted by application restart', finished_at = now()
             WHERE status = 'running'",
        )
        .execute(&self.pool)
        .await?;
        Ok(r.rows_affected())
    }
}
