//! `JobStore` for PostgreSQL. Every transition is a conditional update on the stored status,
//! so two processes cannot both claim or both finish the same job (REAS-STATE-003).

use uuid::Uuid;

use crate::application::ports::{JobOutcome, JobStore, NewJob, StoreResult};
use crate::domain::models::{JobEvent, JobKind, JobStatus, SimulationJob};

use super::PostgresStore;

const JOB_COLS: &str = "id, kind, name, status, params, result, progress, error, seed, app_version, model_version, attempts,
    created_at, started_at, finished_at, updated_at";

impl JobStore for PostgresStore {
    async fn create_job(&self, new: &NewJob<'_>) -> StoreResult<SimulationJob> {
        Ok(sqlx::query_as::<_, SimulationJob>(&format!(
            "INSERT INTO simulation_jobs (id, kind, name, status, params, seed, app_version, model_version)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8) RETURNING {JOB_COLS}"
        ))
        .bind(Uuid::new_v4())
        .bind(new.kind)
        .bind(new.name)
        .bind(JobStatus::Queued)
        .bind(new.params)
        .bind(new.seed)
        .bind(new.app_version)
        .bind(new.model_version)
        .fetch_one(&self.pool)
        .await?)
    }

    async fn job(&self, id: Uuid) -> StoreResult<Option<SimulationJob>> {
        Ok(sqlx::query_as::<_, SimulationJob>(&format!(
            "SELECT {JOB_COLS} FROM simulation_jobs WHERE id = $1"
        ))
        .bind(id)
        .fetch_optional(&self.pool)
        .await?)
    }

    async fn jobs(&self, kind: Option<JobKind>, limit: i64) -> StoreResult<Vec<SimulationJob>> {
        Ok(sqlx::query_as::<_, SimulationJob>(&format!(
            "SELECT {JOB_COLS} FROM simulation_jobs WHERE ($1::text IS NULL OR kind = $1) ORDER BY created_at DESC LIMIT $2"
        ))
        .bind(kind.map(JobKind::as_str))
        .bind(limit)
        .fetch_all(&self.pool)
        .await?)
    }

    async fn scenarios(&self, kind: JobKind) -> StoreResult<Vec<SimulationJob>> {
        Ok(sqlx::query_as::<_, SimulationJob>(&format!(
            "SELECT {JOB_COLS} FROM simulation_jobs WHERE kind = $1 AND name IS NOT NULL AND status = $2
             ORDER BY created_at DESC LIMIT 200"
        ))
        .bind(kind)
        .bind(JobStatus::Completed)
        .fetch_all(&self.pool)
        .await?)
    }

    async fn claim_next_job(&self) -> StoreResult<Option<SimulationJob>> {
        Ok(sqlx::query_as::<_, SimulationJob>(&format!(
            "UPDATE simulation_jobs SET status = $2, started_at = now(), updated_at = now(), attempts = attempts + 1,
                    progress = 0, error = NULL
             WHERE id = (SELECT id FROM simulation_jobs WHERE status = $1 ORDER BY created_at LIMIT 1 FOR UPDATE SKIP LOCKED)
             RETURNING {JOB_COLS}"
        ))
        .bind(JobStatus::Queued)
        .bind(JobStatus::Running)
        .fetch_optional(&self.pool)
        .await?)
    }

    async fn set_job_progress(&self, id: Uuid, progress: f64) -> StoreResult<()> {
        sqlx::query("UPDATE simulation_jobs SET progress = $2, updated_at = now() WHERE id = $1 AND status = $3")
            .bind(id)
            .bind(progress)
            .bind(JobStatus::Running)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn finish_job(&self, id: Uuid, outcome: &JobOutcome) -> StoreResult<bool> {
        let (result, error, progress) = match outcome {
            JobOutcome::Completed(v) => (Some(v), None, 1.0),
            JobOutcome::Failed(e) | JobOutcome::Interrupted(e) | JobOutcome::Cancelled(e) => {
                (None, Some(e.as_str()), -1.0)
            }
        };
        let r = sqlx::query(
            "UPDATE simulation_jobs SET status = $2, result = $3, error = $4,
                    progress = CASE WHEN $5 >= 0 THEN $5 ELSE progress END,
                    finished_at = now(), updated_at = now()
             WHERE id = $1 AND status = $6",
        )
        .bind(id)
        .bind(outcome.status())
        .bind(result)
        .bind(error)
        .bind(progress)
        .bind(JobStatus::Running)
        .execute(&self.pool)
        .await?;
        Ok(r.rows_affected() == 1)
    }

    async fn apply_job_event(&self, id: Uuid, event: JobEvent) -> StoreResult<Option<JobStatus>> {
        for &from in JobStatus::ALL {
            let Ok(to) = from.transition(event) else {
                continue;
            };
            let r = sqlx::query("UPDATE simulation_jobs SET status = $3, updated_at = now() WHERE id = $1 AND status = $2")
                .bind(id)
                .bind(from)
                .bind(to)
                .execute(&self.pool)
                .await?;
            if r.rows_affected() == 1 {
                return Ok(Some(to));
            }
        }
        Ok(None)
    }

    async fn retry_job(&self, id: Uuid) -> StoreResult<bool> {
        let retryable: Vec<JobStatus> = JobStatus::ALL
            .iter()
            .copied()
            .filter(|s| s.can_retry())
            .collect();
        let r = sqlx::query(
            "UPDATE simulation_jobs SET status = $2, error = NULL, progress = 0, result = NULL, started_at = NULL,
                    finished_at = NULL, updated_at = now()
             WHERE id = $1 AND status = ANY($3)",
        )
        .bind(id)
        .bind(JobStatus::Queued)
        .bind(&retryable)
        .execute(&self.pool)
        .await?;
        Ok(r.rows_affected() == 1)
    }

    async fn rename_job(&self, id: Uuid, name: Option<&str>) -> StoreResult<bool> {
        let r =
            sqlx::query("UPDATE simulation_jobs SET name = $2, updated_at = now() WHERE id = $1")
                .bind(id)
                .bind(name)
                .execute(&self.pool)
                .await?;
        Ok(r.rows_affected() == 1)
    }

    async fn delete_job(&self, id: Uuid) -> StoreResult<bool> {
        let r = sqlx::query("DELETE FROM simulation_jobs WHERE id = $1 AND status <> $2")
            .bind(id)
            .bind(JobStatus::Running)
            .execute(&self.pool)
            .await?;
        Ok(r.rows_affected() == 1)
    }

    async fn mark_running_jobs_interrupted(&self) -> StoreResult<u64> {
        let r = sqlx::query(
            "UPDATE simulation_jobs SET status = $2, error = 'interrupted by application restart', updated_at = now()
             WHERE status = $1",
        )
        .bind(JobStatus::Running)
        .bind(JobStatus::Interrupted)
        .execute(&self.pool)
        .await?;
        Ok(r.rows_affected())
    }

    async fn job_counts(&self) -> StoreResult<Vec<(JobStatus, i64)>> {
        Ok(sqlx::query_as(
            "SELECT status, count(*) FROM simulation_jobs GROUP BY status ORDER BY status",
        )
        .fetch_all(&self.pool)
        .await?)
    }
}
