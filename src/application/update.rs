//! One click brings the current season up to date from the results and player source: the
//! results and schedule (the Elo recomputed), then the box scores of every played date not
//! fetched yet and of the last days again, today's injury report and the current rosters.
//!
//! One update runs at a time (`app.update_lock`). The results step holds the results lock
//! and lets it go when it ends; the player step then queues for the player lock, so a
//! backfill holding it is not refused but waited for: the backfill loop lets go between
//! seasons, and a queued waiter is served before the loop can take the lock again. Each
//! step publishes its progress where the step alone would (`app.sync_phase()`,
//! `app.player_phase()`); `UpdatePhase` says which step is on and how each ended.

use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde::Serialize;
use tokio::sync::OwnedMutexGuard;
use tracing::info;

use crate::domain::ids::SeasonYear;

use super::continuation::ContinuationUseCaseError;
use super::players::{self, PlayerSyncError, PlayerSyncKind};
use super::ports::{PlayerSource, ResultsSource, SourceError};
use super::results_sync::{self, Trigger};
use super::scheduler::current_season;
use super::state::App;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum UpdateStep {
    /// Results and schedule, and the Elo recomputed.
    Results,
    /// Waiting for a backfill to let go of the player lock.
    WaitingForPlayers,
    /// Box scores, today's injury report and the rosters.
    Players,
}

/// How one step ended: `None` when it completed, else what stopped it.
pub type StepError = Option<String>;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "phase", rename_all = "snake_case")]
pub enum UpdatePhase {
    Idle,
    Running {
        season: SeasonYear,
        step: UpdateStep,
        started_at: DateTime<Utc>,
        /// How the results step ended, once it has: carried so the page can say it early
        /// and so a stop in the player step keeps it.
        #[serde(skip_serializing_if = "Option::is_none")]
        results_error: StepError,
    },
    Done {
        season: SeasonYear,
        started_at: DateTime<Utc>,
        finished_at: DateTime<Utc>,
        results_error: StepError,
        players_error: StepError,
        /// The player step was refused for the plan (status 401 or 403 before any date
        /// was stored, with the same key accepted for the results): the plan serves no box
        /// scores, injury reports or rosters. Not counted as an error.
        players_note: Option<String>,
    },
}

impl UpdatePhase {
    pub fn is_running(&self) -> bool {
        matches!(self, UpdatePhase::Running { .. })
    }
}

#[derive(Debug, thiserror::Error)]
pub enum StartError {
    #[error("updates are not configured: set BALLDONTLIE_API_KEY and restart")]
    NotConfigured,
    #[error("an update is already running")]
    AlreadyRunning,
    #[error("a results sync or an archive import is running; try again when it ends")]
    ResultsRunning,
}

/// Said in place of an error when the player step's source refuses the plan.
const PLAN_NOTE: &str =
    "the BALLDONTLIE plan serves no box scores, injury reports or rosters (an ALL-STAR tier does)";

/// Run the update of `season`. The caller holds the update lock and hands over the
/// results lock, which is let go when the results step ends; the player step then waits
/// for the player lock. The player step runs whatever the results step did: the injury
/// report and the rosters do not depend on it, and a date whose game the results step
/// did not mark played waits for the next run.
pub async fn run<R: ResultsSource, P: PlayerSource>(
    app: &App,
    results: &R,
    players_source: &P,
    season: SeasonYear,
    started_at: DateTime<Utc>,
    results_lock: OwnedMutexGuard<()>,
) -> UpdatePhase {
    let results_outcome = results_sync::run_chained(app, results, season, Trigger::Manual).await;
    drop(results_lock);
    let results_rejected = matches!(
        &results_outcome,
        Err(ContinuationUseCaseError::Source(SourceError::Rejected {
            status: 401 | 403,
            ..
        }))
    );
    let results_error = results_outcome.err().map(|e| e.to_string());
    let step = |step| UpdatePhase::Running {
        season,
        step,
        started_at,
        results_error: results_error.clone(),
    };
    app.set_update_phase(step(UpdateStep::Players));
    let players_lock = match app.player_lock.clone().try_lock_owned() {
        Ok(guard) => guard,
        Err(_) => {
            app.set_update_phase(step(UpdateStep::WaitingForPlayers));
            let guard = app.player_lock.clone().lock_owned().await;
            app.set_update_phase(step(UpdateStep::Players));
            guard
        }
    };
    let outcome = players::run_tracked(
        app,
        players_source,
        PlayerSyncKind::Daily { season },
        Trigger::Manual,
    )
    .await;
    drop(players_lock);
    let (players_error, players_note) = player_outcome(results_rejected, &outcome);
    let done = UpdatePhase::Done {
        season,
        started_at,
        finished_at: Utc::now(),
        results_error,
        players_error,
        players_note,
    };
    info!(%season, ?done, "update finished");
    app.set_update_phase(done.clone());
    done
}

/// How the player step ended, as an error or a note. A 401 or 403 is the plan, not a
/// fault, only when the same key was accepted for the results and the run stored no date
/// before the refusal; otherwise it is an error (a bad key, a block, a partial run).
fn player_outcome(
    results_rejected: bool,
    outcome: &Result<players::PlayerSyncOutcome, Box<players::Interrupted>>,
) -> (StepError, Option<String>) {
    match outcome {
        Ok(_) => (None, None),
        Err(e) if !results_rejected && e.done.dates_fetched == 0 && refused_by_plan(&e.error) => {
            (None, Some(PLAN_NOTE.to_string()))
        }
        Err(e) => (Some(e.to_string()), None),
    }
}

/// A player run the source refused for the plan, not for a fault.
fn refused_by_plan(error: &PlayerSyncError) -> bool {
    matches!(
        error,
        PlayerSyncError::Source(SourceError::Rejected {
            status: 401 | 403,
            ..
        })
    )
}

/// Start the update of the current season in the background; returns the season at once.
/// Refused while another update runs or while a results sync holds its lock; a player
/// sync holding its lock is waited for. Poll `app.update_phase()`.
pub fn start(app: Arc<App>) -> Result<SeasonYear, StartError> {
    if app.results.is_none() {
        return Err(StartError::NotConfigured);
    }
    let update_guard = app
        .update_lock
        .clone()
        .try_lock_owned()
        .map_err(|_| StartError::AlreadyRunning)?;
    let results_guard = app
        .sync_lock
        .clone()
        .try_lock_owned()
        .map_err(|_| StartError::ResultsRunning)?;
    let season = current_season(app.today());
    let started_at = Utc::now();
    app.set_update_phase(UpdatePhase::Running {
        season,
        step: UpdateStep::Results,
        started_at,
        results_error: None,
    });
    tokio::spawn(async move {
        let _update = update_guard;
        // The run in a task of its own, so a panic in it still ends the phase.
        let inner = {
            let app = app.clone();
            tokio::spawn(async move {
                match app.results.as_ref() {
                    Some(source) => {
                        run(&app, source, source, season, started_at, results_guard).await;
                    }
                    None => drop(results_guard),
                }
            })
        };
        let ended = inner.await;
        if let UpdatePhase::Running {
            step,
            results_error,
            ..
        } = app.update_phase()
        {
            let stopped = Some(match ended {
                Err(e) => format!("the update stopped unexpectedly: {e}"),
                Ok(()) => "the update stopped unexpectedly".to_string(),
            });
            let at_results = step == UpdateStep::Results;
            app.set_update_phase(UpdatePhase::Done {
                season,
                started_at,
                finished_at: Utc::now(),
                results_error: if at_results {
                    stopped.clone()
                } else {
                    results_error
                },
                players_error: if at_results { None } else { stopped },
                players_note: None,
            });
        }
    });
    Ok(season)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_phase_serializes_its_step_and_each_steps_outcome() {
        let t = Utc::now();
        let running = UpdatePhase::Running {
            season: SeasonYear::of(2027),
            step: UpdateStep::Players,
            started_at: t,
            results_error: None,
        };
        assert!(running.is_running());
        let v = serde_json::to_value(&running).unwrap();
        assert_eq!(
            (v["phase"].as_str(), v["step"].as_str()),
            (Some("running"), Some("players"))
        );
        assert_eq!(v["season"], 2027);
        let done = UpdatePhase::Done {
            season: SeasonYear::of(2027),
            started_at: t,
            finished_at: t,
            results_error: None,
            players_error: Some("the source rejected the key".into()),
            players_note: None,
        };
        assert!(!done.is_running());
        let v = serde_json::to_value(&done).unwrap();
        assert_eq!(v["phase"], "done");
        assert!(v["results_error"].is_null());
        assert_eq!(v["players_error"], "the source rejected the key");
        assert_eq!(
            serde_json::to_value(UpdatePhase::Idle).unwrap()["phase"],
            "idle"
        );
        let waiting = UpdatePhase::Running {
            season: SeasonYear::of(2027),
            step: UpdateStep::WaitingForPlayers,
            started_at: t,
            results_error: Some("no games".into()),
        };
        assert_eq!(
            serde_json::to_value(&waiting).unwrap()["step"],
            "waiting_for_players"
        );
    }

    #[test]
    fn a_plan_without_player_data_is_a_note_not_an_error() {
        let rejected = |status| {
            PlayerSyncError::Source(SourceError::Rejected {
                status,
                message: "tier".into(),
            })
        };
        let refused = |status, dates| {
            Err(Box::new(players::Interrupted {
                done: players::PlayerSyncOutcome {
                    dates_fetched: dates,
                    ..Default::default()
                },
                dates_total: 5,
                error: rejected(status),
            }))
        };
        // The key worked for the results and nothing was stored: the plan.
        let (error, note) = player_outcome(false, &refused(401, 0));
        assert_eq!((error, note.as_deref()), (None, Some(PLAN_NOTE)));
        // The results were refused too: the key, an error.
        let (error, note) = player_outcome(true, &refused(401, 0));
        assert!(error.is_some() && note.is_none());
        // Dates stored before a 403: a fault, not the plan.
        let (error, note) = player_outcome(false, &refused(403, 2));
        assert!(error.is_some() && note.is_none());
        let (error, note) = player_outcome(false, &refused(404, 0));
        assert!(error.is_some() && note.is_none());
        let ok: Result<players::PlayerSyncOutcome, Box<players::Interrupted>> =
            Ok(Default::default());
        assert_eq!(player_outcome(false, &ok), (None, None));
        assert!(refused_by_plan(&rejected(401)));
        assert!(refused_by_plan(&rejected(403)));
        assert!(!refused_by_plan(&rejected(404)));
        assert!(!refused_by_plan(&PlayerSyncError::Source(
            SourceError::RateLimited(3)
        )));
    }
}
