//! Background job runner.
//!
//! A job is any type implementing [`JobSpec`]; the blanket [`Submit`] impl gives every spec a
//! `submit` method that validates, serialises and queues it through the `JobStore` port. The
//! runner claims queued jobs one at a time, executes the CPU-bound part on the blocking pool
//! (Rayon inside), streams progress to the store, and records completion, failure,
//! cancellation or interruption through the `JobStatus` state machine.

pub mod specs;

use std::future::Future;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use serde::de::DeserializeOwned;
use serde::Serialize;
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::config::APP_VERSION;
use crate::domain::models::{JobEvent, JobKind, JobStatus, SimulationJob};
use crate::domain::simulation::progress::Progress;
use crate::domain::validation::ValidationError;

use super::ports::{JobOutcome, JobStore, NewJob, StoreError};
use super::state::App;

#[derive(Debug, Clone)]
pub struct Limits {
    pub max_iterations: u64,
    /// Upper bound on the estimated work of one job (elementary draws).
    pub max_work_units: u64,
}

impl Limits {
    /// Reject a job whose estimated work (`iterations × per_iteration`) exceeds the budget.
    pub fn check_work(&self, iterations: u64, per_iteration: u64) -> Result<(), ValidationError> {
        let work = iterations.saturating_mul(per_iteration);
        if work > self.max_work_units {
            return Err(ValidationError::new(
                "iterations",
                format!(
                    "estimated work {work} exceeds the budget of {} units; reduce iterations or the problem size",
                    self.max_work_units
                ),
            ));
        }
        Ok(())
    }
}

pub struct JobContext {
    pub app: Arc<App>,
    pub job_id: Uuid,
    pub progress: ProgressHandle,
}

/// A progress sink shared between the engine thread and the store flusher.
#[derive(Clone, Default)]
pub struct ProgressHandle {
    latest_bits: Arc<AtomicU64>,
    cancel: Arc<AtomicBool>,
}

impl ProgressHandle {
    pub fn new(cancel: Arc<AtomicBool>) -> Self {
        Self {
            latest_bits: Arc::new(AtomicU64::new(0)),
            cancel,
        }
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
        self.latest_bits
            .store(fraction.to_bits(), Ordering::Relaxed);
        !self.is_cancelled()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum JobError {
    #[error("invalid parameters: {0}")]
    Invalid(#[from] ValidationError),
    #[error("stored parameters do not match the job kind: {0}")]
    Params(String),
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error("{0}")]
    Execution(String),
}

impl From<anyhow::Error> for JobError {
    fn from(e: anyhow::Error) -> Self {
        JobError::Execution(e.to_string())
    }
}

pub trait JobSpec: Serialize + DeserializeOwned + Send + Sync + 'static {
    const KIND: JobKind;
    const MODEL_VERSION: &'static str;
    type Output: Serialize + Send;

    fn validate(&self, limits: &Limits) -> Result<(), ValidationError>;

    fn seed(&self) -> Option<i64> {
        None
    }

    fn execute(
        self,
        ctx: JobContext,
    ) -> impl Future<Output = Result<Self::Output, JobError>> + Send;
}

/// Blanket extension: every `JobSpec` can be submitted through any `JobStore`.
pub trait Submit: JobSpec {
    fn submit<S: JobStore>(
        self,
        store: &S,
        limits: &Limits,
        name: Option<String>,
    ) -> impl Future<Output = Result<SimulationJob, JobError>>
    where
        Self: Sized,
    {
        async move {
            self.validate(limits)?;
            let params =
                serde_json::to_value(&self).map_err(|e| JobError::Params(e.to_string()))?;
            let job = store
                .create_job(&NewJob {
                    kind: Self::KIND,
                    name: name.as_deref(),
                    params: &params,
                    seed: self.seed(),
                    app_version: APP_VERSION,
                    model_version: Self::MODEL_VERSION,
                })
                .await?;
            Ok(job)
        }
    }
}

impl<T: JobSpec> Submit for T {}

/// Execute one claimed job of type `T` end to end and record its outcome.
async fn run_job<T: JobSpec>(app: Arc<App>, job: SimulationJob) -> Result<(), JobError> {
    let spec: T =
        serde_json::from_value(job.params.clone()).map_err(|e| JobError::Params(e.to_string()))?;
    let cancel = app.cancel_flag(job.id);
    let progress = ProgressHandle::new(cancel.clone());
    let ctx = JobContext {
        app: app.clone(),
        job_id: job.id,
        progress: progress.clone(),
    };
    // Flush progress to the store while the job runs.
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
                    if let Err(e) = app.store.set_job_progress(id, p).await {
                        warn!(error = %e, "progress update failed");
                    }
                }
            }
        })
    };
    let cancel_on_shutdown = {
        let shutdown = app.shutdown.clone();
        let cancel = cancel.clone();
        tokio::spawn(async move {
            shutdown.cancelled().await;
            cancel.store(true, Ordering::Relaxed);
        })
    };
    let result = spec.execute(ctx).await;
    flusher.abort();
    cancel_on_shutdown.abort();
    app.clear_cancel_flag(job.id);
    let outcome = match result {
        Ok(output) if !cancel.load(Ordering::Relaxed) => JobOutcome::Completed(
            serde_json::to_value(&output).map_err(|e| JobError::Params(e.to_string()))?,
        ),
        Ok(_) if app.shutdown.is_cancelled() => {
            JobOutcome::Interrupted("interrupted by shutdown".into())
        }
        Ok(_) => JobOutcome::Cancelled("cancelled by user".into()),
        Err(e) => JobOutcome::Failed(e.to_string()),
    };
    let applied = app.store.finish_job(job.id, &outcome).await?;
    match (&outcome, applied) {
        (_, false) => {
            warn!(job = %job.id, "job was no longer running when its outcome arrived; outcome discarded")
        }
        (JobOutcome::Completed(_), true) => info!(job = %job.id, kind = %job.kind, "job completed"),
        (JobOutcome::Interrupted(_), true) => {
            warn!(job = %job.id, "job interrupted by shutdown; retry to resume")
        }
        (JobOutcome::Cancelled(_), true) => info!(job = %job.id, "job cancelled"),
        (JobOutcome::Failed(e), true) => error!(job = %job.id, error = %e, "job failed"),
    }
    Ok(())
}

/// Dispatch on the stored kind.
async fn dispatch(app: Arc<App>, job: SimulationJob) -> Result<(), JobError> {
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
        match app.store.claim_next_job().await {
            Ok(Some(job)) => {
                info!(job = %job.id, kind = %job.kind, attempt = job.attempts, "job claimed");
                if let Err(e) = dispatch(app.clone(), job.clone()).await {
                    error!(job = %job.id, error = %e, "job bookkeeping failed");
                    let _ = app
                        .store
                        .finish_job(job.id, &JobOutcome::Failed(e.to_string()))
                        .await;
                }
                continue;
            }
            Ok(None) => {}
            Err(e) => warn!(error = %e, "could not poll jobs; store unavailable?"),
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
pub async fn cancel(app: &App, id: Uuid) -> Result<Option<JobStatus>, StoreError> {
    let running_here = app
        .cancel_flags
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(&id)
        .cloned();
    if let Some(flag) = running_here {
        flag.store(true, Ordering::Relaxed);
        return Ok(Some(JobStatus::Running));
    }
    // Not running in this process: the state machine decides (only Queued -> Cancelled applies).
    app.store.apply_job_event(id, JobEvent::Cancel).await
}

pub async fn retry(app: &App, id: Uuid) -> Result<Option<JobStatus>, StoreError> {
    let requeued = app.store.retry_job(id).await?;
    if requeued {
        app.job_notify.notify_one();
    }
    Ok(requeued.then_some(JobStatus::Queued))
}
