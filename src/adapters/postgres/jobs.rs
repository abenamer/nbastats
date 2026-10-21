use serde_json::Value;
use uuid::Uuid;

use crate::domain::models::{JobKind, JobStatus, SimulationJob};

use super::Pool;

const JOB_COLS: &str = "id, kind, name, status, params, result, progress, error, seed, app_version, model_version, attempts,
    created_at, started_at, finished_at, updated_at";

pub async fn create(
    pool: &Pool,
    kind: JobKind,
    name: Option<&str>,
    params: &Value,
    seed: Option<i64>,
    app_version: &str,
    model_version: &str,
) -> anyhow::Result<SimulationJob> {
    let id = Uuid::new_v4();
    Ok(sqlx::query_as::<_, SimulationJob>(&format!(
        "INSERT INTO simulation_jobs (id, kind, name, status, params, seed, app_version, model_version)
         VALUES ($1, $2, $3, 'queued', $4, $5, $6, $7) RETURNING {JOB_COLS}"
    ))
    .bind(id)
    .bind(kind)
    .bind(name)
    .bind(params)
    .bind(seed)
    .bind(app_version)
    .bind(model_version)
    .fetch_one(pool)
    .await?)
}

pub async fn get(pool: &Pool, id: Uuid) -> anyhow::Result<Option<SimulationJob>> {
    Ok(sqlx::query_as::<_, SimulationJob>(&format!("SELECT {JOB_COLS} FROM simulation_jobs WHERE id = $1"))
        .bind(id)
        .fetch_optional(pool)
        .await?)
}

pub async fn list(pool: &Pool, kind: Option<JobKind>, limit: i64) -> anyhow::Result<Vec<SimulationJob>> {
    Ok(sqlx::query_as::<_, SimulationJob>(&format!(
        "SELECT {JOB_COLS} FROM simulation_jobs WHERE ($1::text IS NULL OR kind = $1)
         ORDER BY created_at DESC LIMIT $2"
    ))
    .bind(kind.map(|k| k.as_str()))
    .bind(limit)
    .fetch_all(pool)
    .await?)
}

/// Named, completed jobs of a kind ("scenarios").
pub async fn scenarios(pool: &Pool, kind: JobKind) -> anyhow::Result<Vec<SimulationJob>> {
    Ok(sqlx::query_as::<_, SimulationJob>(&format!(
        "SELECT {JOB_COLS} FROM simulation_jobs WHERE kind = $1 AND name IS NOT NULL AND status = 'completed'
         ORDER BY created_at DESC LIMIT 200"
    ))
    .bind(kind)
    .fetch_all(pool)
    .await?)
}

/// Atomically claim the oldest queued job.
pub async fn claim_next(pool: &Pool) -> anyhow::Result<Option<SimulationJob>> {
    Ok(sqlx::query_as::<_, SimulationJob>(&format!(
        "UPDATE simulation_jobs SET status = 'running', started_at = now(), updated_at = now(), attempts = attempts + 1,
                progress = 0, error = NULL
         WHERE id = (SELECT id FROM simulation_jobs WHERE status = 'queued' ORDER BY created_at LIMIT 1 FOR UPDATE SKIP LOCKED)
         RETURNING {JOB_COLS}"
    ))
    .fetch_optional(pool)
    .await?)
}

pub async fn set_progress(pool: &Pool, id: Uuid, progress: f64) -> anyhow::Result<()> {
    sqlx::query("UPDATE simulation_jobs SET progress = $2, updated_at = now() WHERE id = $1 AND status = 'running'")
        .bind(id)
        .bind(progress)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn complete(pool: &Pool, id: Uuid, result: &Value) -> anyhow::Result<()> {
    sqlx::query(
        "UPDATE simulation_jobs SET status = 'completed', result = $2, progress = 1, finished_at = now(), updated_at = now()
         WHERE id = $1",
    )
    .bind(id)
    .bind(result)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn fail(pool: &Pool, id: Uuid, error: &str) -> anyhow::Result<()> {
    sqlx::query("UPDATE simulation_jobs SET status = 'failed', error = $2, finished_at = now(), updated_at = now() WHERE id = $1")
        .bind(id)
        .bind(error)
        .execute(pool)
        .await?;
    Ok(())
}

/// Apply a lifecycle event in the database, enforcing the state machine on the stored status
/// (the `WHERE status = ANY(...)` clause only allows transitions `JobStatus::transition` permits).
pub async fn apply_event(pool: &Pool, id: Uuid, event: crate::domain::models::JobEvent) -> anyhow::Result<Option<JobStatus>> {
    let allowed: Vec<(JobStatus, JobStatus)> =
        JobStatus::ALL.iter().filter_map(|&from| from.transition(event).ok().map(|to| (from, to))).collect();
    for (from, to) in allowed {
        let r = sqlx::query("UPDATE simulation_jobs SET status = $3, updated_at = now() WHERE id = $1 AND status = $2")
            .bind(id)
            .bind(from)
            .bind(to)
            .execute(pool)
            .await?;
        if r.rows_affected() == 1 {
            return Ok(Some(to));
        }
    }
    Ok(None)
}

pub async fn set_status(pool: &Pool, id: Uuid, status: JobStatus, error: Option<&str>) -> anyhow::Result<()> {
    sqlx::query("UPDATE simulation_jobs SET status = $2, error = COALESCE($3, error), updated_at = now() WHERE id = $1")
        .bind(id)
        .bind(status)
        .bind(error)
        .execute(pool)
        .await?;
    Ok(())
}

/// Called at startup: any job left in 'running' by a previous process was interrupted.
/// Its configuration is kept so it can be retried safely.
pub async fn mark_interrupted(pool: &Pool) -> anyhow::Result<u64> {
    let r = sqlx::query(
        "UPDATE simulation_jobs SET status = 'interrupted', error = 'interrupted by application restart', updated_at = now()
         WHERE status = 'running'",
    )
    .execute(pool)
    .await?;
    Ok(r.rows_affected())
}

/// Re-queue an interrupted, failed or cancelled job (keeps its parameters and seed).
pub async fn retry(pool: &Pool, id: Uuid) -> anyhow::Result<bool> {
    let r = sqlx::query(
        "UPDATE simulation_jobs SET status = 'queued', error = NULL, progress = 0, result = NULL, started_at = NULL,
                finished_at = NULL, updated_at = now()
         WHERE id = $1 AND status IN ('interrupted', 'failed', 'cancelled')",
    )
    .bind(id)
    .execute(pool)
    .await?;
    Ok(r.rows_affected() == 1)
}

pub async fn rename(pool: &Pool, id: Uuid, name: Option<&str>) -> anyhow::Result<bool> {
    let r = sqlx::query("UPDATE simulation_jobs SET name = $2, updated_at = now() WHERE id = $1")
        .bind(id)
        .bind(name)
        .execute(pool)
        .await?;
    Ok(r.rows_affected() == 1)
}

pub async fn delete(pool: &Pool, id: Uuid) -> anyhow::Result<bool> {
    let r = sqlx::query("DELETE FROM simulation_jobs WHERE id = $1 AND status <> 'running'")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(r.rows_affected() == 1)
}

pub async fn counts_by_status(pool: &Pool) -> anyhow::Result<Vec<(String, i64)>> {
    Ok(sqlx::query_as("SELECT status, count(*) FROM simulation_jobs GROUP BY status ORDER BY status")
        .fetch_all(pool)
        .await?)
}

/// Most recent completed job of a kind whose params contain the given JSON (containment query).
pub async fn latest_completed_matching(pool: &Pool, kind: JobKind, params_subset: &Value) -> anyhow::Result<Option<SimulationJob>> {
    Ok(sqlx::query_as::<_, SimulationJob>(&format!(
        "SELECT {JOB_COLS} FROM simulation_jobs WHERE kind = $1 AND status = 'completed' AND params @> $2
         ORDER BY finished_at DESC LIMIT 1"
    ))
    .bind(kind)
    .bind(params_subset)
    .fetch_optional(pool)
    .await?)
}

