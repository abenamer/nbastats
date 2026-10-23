//! One results sync at a time, observable while it runs.
//!
//! `SyncPhase` is the state machine the Data page polls: idle, fetching (with live request
//! and game counts), done or failed. `start` runs a sync as a background task; the scheduler
//! runs through the same lock so a manual click and the daily run never overlap.

use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde::Serialize;
use tracing::{error, info};

use crate::domain::ids::SeasonYear;

use super::continuation::{self, ContinuationUseCaseError};
use super::imports::ImportOutcome;
use super::ports::{FetchProgress, ResultsSource};
use super::state::App;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Trigger {
    Manual,
    Scheduled,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "phase", rename_all = "snake_case")]
pub enum SyncPhase {
    Idle,
    Fetching {
        season: SeasonYear,
        trigger: Trigger,
        started_at: DateTime<Utc>,
        requests: u32,
        games: usize,
        /// Seconds until the next request may start (pacing or a 429 back-off).
        waiting_secs: Option<u64>,
        rate_limited: bool,
    },
    Done {
        season: SeasonYear,
        trigger: Trigger,
        started_at: DateTime<Utc>,
        finished_at: DateTime<Utc>,
        outcome: ImportOutcome,
    },
    Failed {
        season: SeasonYear,
        trigger: Trigger,
        started_at: DateTime<Utc>,
        finished_at: DateTime<Utc>,
        error: String,
    },
}

impl SyncPhase {
    pub fn is_running(&self) -> bool {
        matches!(self, SyncPhase::Fetching { .. })
    }

    fn with_progress(&self, p: FetchProgress) -> SyncPhase {
        match self {
            SyncPhase::Fetching {
                season,
                trigger,
                started_at,
                ..
            } => SyncPhase::Fetching {
                season: *season,
                trigger: *trigger,
                started_at: *started_at,
                requests: p.requests,
                games: p.games,
                waiting_secs: p.waiting.map(|d| d.as_secs()),
                rate_limited: p.rate_limited,
            },
            other => other.clone(),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum StartError {
    #[error("results sync is not configured: set BALLDONTLIE_API_KEY and restart")]
    NotConfigured,
    #[error("a results sync is already running")]
    AlreadyRunning,
}

/// Run one sync while the caller holds the sync lock, publishing progress to `app.sync_phase`.
pub async fn run_tracked<R: ResultsSource>(
    app: &App,
    source: &R,
    season: SeasonYear,
    trigger: Trigger,
) -> Result<ImportOutcome, ContinuationUseCaseError> {
    let started_at = Utc::now();
    app.set_sync_phase(SyncPhase::Fetching {
        season,
        trigger,
        started_at,
        requests: 0,
        games: 0,
        waiting_secs: None,
        rate_limited: false,
    });
    let progress = |p: FetchProgress| {
        let current = app.sync_phase();
        app.set_sync_phase(current.with_progress(p));
    };
    let result =
        continuation::sync_results(&app.store, source, season, app.today(), &progress).await;
    let finished_at = Utc::now();
    match &result {
        Ok(outcome) => {
            app.invalidate_cache();
            info!(
                %season, ?trigger,
                inserted = outcome.games_inserted,
                updated = outcome.games_updated,
                unchanged = outcome.games_unchanged,
                "results sync completed"
            );
            app.set_sync_phase(SyncPhase::Done {
                season,
                trigger,
                started_at,
                finished_at,
                outcome: outcome.clone(),
            });
        }
        Err(e) => {
            error!(%season, ?trigger, error = %e, "results sync failed");
            app.set_sync_phase(SyncPhase::Failed {
                season,
                trigger,
                started_at,
                finished_at,
                error: e.to_string(),
            });
        }
    }
    result
}

/// Start a manual sync in the background. Returns at once; poll `app.sync_phase()`.
pub fn start(app: Arc<App>, season: SeasonYear) -> Result<(), StartError> {
    if app.results.is_none() {
        return Err(StartError::NotConfigured);
    }
    let guard = app
        .sync_lock
        .clone()
        .try_lock_owned()
        .map_err(|_| StartError::AlreadyRunning)?;
    tokio::spawn(async move {
        let _guard = guard;
        if let Some(source) = app.results.as_ref() {
            let _ = run_tracked(&app, source, season, Trigger::Manual).await;
        }
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn progress_only_updates_a_running_phase() {
        let t = Utc::now();
        let running = SyncPhase::Fetching {
            season: SeasonYear::of(2026),
            trigger: Trigger::Manual,
            started_at: t,
            requests: 0,
            games: 0,
            waiting_secs: None,
            rate_limited: false,
        };
        let p = FetchProgress {
            requests: 3,
            games: 300,
            waiting: Some(Duration::from_secs(11)),
            rate_limited: true,
        };
        assert_eq!(
            running.with_progress(p),
            SyncPhase::Fetching {
                season: SeasonYear::of(2026),
                trigger: Trigger::Manual,
                started_at: t,
                requests: 3,
                games: 300,
                waiting_secs: Some(11),
                rate_limited: true,
            }
        );
        assert_eq!(SyncPhase::Idle.with_progress(p), SyncPhase::Idle);
        assert!(running.is_running() && !SyncPhase::Idle.is_running());
        let json = serde_json::to_value(&running).unwrap();
        assert_eq!(json["phase"], "fetching");
        assert_eq!(json["trigger"], "manual");
    }
}
