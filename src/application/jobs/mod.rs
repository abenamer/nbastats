//! Background job runner.
//!
//! A job is any type implementing [`JobSpec`]; the blanket [`Submit`] impl gives every spec a
//! `submit` method that validates, serialises and queues it. The runner claims queued jobs
//! one at a time, executes the CPU-bound part on the blocking pool (Rayon inside), streams
//! progress to PostgreSQL, and records completion, failure, cancellation or interruption
//! through the `JobStatus` state machine.

pub mod specs;

use std::future::Future;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use serde::de::DeserializeOwned;
use serde::Serialize;
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::application::state::App;
use crate::config::APP_VERSION;
use crate::adapters::postgres as db;
use crate::domain::models::{JobEvent, JobKind, JobStatus, SimulationJob};
use crate::domain::simulation::progress::Progress;

#[derive(Debug, Clone)]
pub struct Limits {
    pub max_iterations: u64,
}

pub struct JobContext {
    pub app: Arc<App>,
    pub job_id: Uuid,
    pub progress: ProgressHandle,
}

/// A progress sink shared between the engine thread and the database flusher.
#[derive(Clone, Default)]
pub struct ProgressHandle {
    latest_bits: Arc<AtomicU64>,
    cancel: Arc<AtomicBool>,
}

impl ProgressHandle {
    pub fn new(cancel: Arc<AtomicBool>) -> Self {
        Self { latest_bits: Arc::new(AtomicU64::new(0)), cancel }
    }
    pub fn latest(&self) -> f64 {
        f64::from_bits(self.latest_bits.load(Ordering::Relaxed))
    }
    pub fn is_cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }
}

impl Progress for ProgressHandle {
    fn report(&self, fraction: f64) -> bool {
        self.latest_bits.store(fraction.to_bits(), Ordering::Relaxed);
        !self.is_cancelled()
    }
}

pub trait JobSpec: Serialize + DeserializeOwned + Send + Sync + 'static {
    const KIND: JobKind;
    const MODEL_VERSION: &'static str;
    type Output: Serialize + Send;

    fn validate(&self, limits: &Limits) -> Result<(), String>;

    fn seed(&self) -> Option<i64> {
        None
    }

    fn execute(self, ctx: JobContext) -> impl Future<Output = anyhow::Result<Self::Output>> + Send;
}

#[derive(Debug, thiserror::Error)]
pub enum SubmitError {
    #[error("invalid parameters: {0}")]
    Invalid(String),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

/// Blanket extension: every `JobSpec` can be submitted.
pub trait Submit: JobSpec {
    fn submit(self, app: &App, name: Option<String>) -> impl Future<Output = Result<SimulationJob, SubmitError>> + Send
    where
        Self: Sized,
    {
        async move {
            self.validate(&Limits { max_iterations: app.config.max_iterations }).map_err(SubmitError::Invalid)?;
            let params = serde_json::to_value(&self).map_err(|e| SubmitError::Other(e.into()))?;
            let job = db::jobs::create(&app.pool, Self::KIND, name.as_deref(), &params, self.seed(), APP_VERSION, Self::MODEL_VERSION)
                .await?;
            app.job_notify.notify_one();
            Ok(job)
        }
    }
}

impl<T: JobSpec> Submit for T {}

/// Execute one claimed job of type `T` end to end.
async fn run_job<T: JobSpec>(app: Arc<App>, job: SimulationJob) -> anyhow::Result<()> {
    let spec: T = serde_json::from_value(job.params.clone())?;
    let cancel = app.cancel_flag(job.id);
    let progress = ProgressHandle::new(cancel.clone());
    let ctx = JobContext { app: app.clone(), job_id: job.id, progress: progress.clone() };
    // Flush progress to the database while the job runs.
    let flusher = {
        let app = app.clone();
        let progress = progress.clone();
        let id = job.id;
        tokio::spawn(async move {
            let mut last = -1.0;
            loop {
                tokio::time::sleep(Duration::from_millis(500)).await;
                let p = progress.latest();
                if (p - last).abs() > 0.005 {
                    last = p;
                    if let Err(e) = db::jobs::set_progress(&app.pool, id, p).await {
                        warn!(error = %e, "progress update failed");
                    }
                }
            }
        })
    };
    let shutdown = app.shutdown.clone();
    let cancel_on_shutdown = {
        let cancel = cancel.clone();
        tokio::spawn(async move {
            shutdown.cancelled().await;
            cancel.store(true, Ordering::Relaxed);
        })
    };
    let outcome = spec.execute(ctx).await;
    flusher.abort();
    cancel_on_shutdown.abort();
    app.clear_cancel_flag(job.id);
    match outcome {
        Ok(output) if !cancel.load(Ordering::Relaxed) => {
            let value = serde_json::to_value(&output)?;
            db::jobs::complete(&app.pool, job.id, &value).await?;
            info!(job = %job.id, kind = %job.kind, "job completed");
        }
        Ok(_) if app.shutdown.is_cancelled() => {
            db::jobs::set_status(&app.pool, job.id, JobStatus::Interrupted, Some("interrupted by shutdown")).await?;
            warn!(job = %job.id, "job interrupted by shutdown; retry to resume");
        }
        Ok(_) => {
            db::jobs::set_status(&app.pool, job.id, JobStatus::Cancelled, Some("cancelled by user")).await?;
            info!(job = %job.id, "job cancelled");
        }
        Err(e) => {
            db::jobs::fail(&app.pool, job.id, &e.to_string()).await?;
            error!(job = %job.id, error = %e, "job failed");
        }
    }
    Ok(())
}

/// Dispatch on the stored kind.
async fn dispatch(app: Arc<App>, job: SimulationJob) -> anyhow::Result<()> {
    match job.kind {
        JobKind::Alpha => run_job::<specs::AlphaJob>(app, job).await,
        JobKind::Playoff => run_job::<specs::PlayoffJob>(app, job).await,
        JobKind::Season => run_job::<specs::SeasonJob>(app, job).await,
        JobKind::Backtest => run_job::<specs::BacktestJob>(app, job).await,
        JobKind::Dynasty => run_job::<specs::DynastyJob>(app, job).await,
    }
}

/// The runner loop. Returns when the application shuts down and no job is running.
pub async fn runner(app: Arc<App>) {
    info!("job runner started");
    loop {
        if app.shutdown.is_cancelled() {
            break;
        }
        match db::jobs::claim_next(&app.pool).await {
            Ok(Some(job)) => {
                info!(job = %job.id, kind = %job.kind, attempt = job.attempts, "job claimed");
                if let Err(e) = dispatch(app.clone(), job.clone()).await {
                    error!(job = %job.id, error = %e, "job bookkeeping failed");
                    let _ = db::jobs::fail(&app.pool, job.id, &e.to_string()).await;
                }
                continue;
            }
            Ok(None) => {}
            Err(e) => {
                warn!(error = %e, "could not poll jobs; database unavailable?");
            }
        }
        tokio::select! {
            _ = app.job_notify.notified() => {}
            _ = tokio::time::sleep(Duration::from_secs(2)) => {}
            _ = app.shutdown.cancelled() => break,
        }
    }
    info!("job runner stopped");
}

/// Request cancellation of a running job (or cancel a queued one directly).
pub async fn cancel(app: &App, id: Uuid) -> anyhow::Result<Option<JobStatus>> {
    if let Some(flag) = app.cancel_flags.lock().expect("cancel lock").get(&id) {
        flag.store(true, Ordering::Relaxed);
        return Ok(Some(JobStatus::Running));
    }
    // Not running in this process: the state machine decides (only Queued -> Cancelled applies).
    db::jobs::apply_event(&app.pool, id, JobEvent::Cancel).await
}

pub async fn retry(app: &App, id: Uuid) -> anyhow::Result<Option<JobStatus>> {
    let requeued = db::jobs::retry(&app.pool, id).await?;
    if requeued {
        app.job_notify.notify_one();
    }
    Ok(requeued.then_some(JobStatus::Queued))
}
