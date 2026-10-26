//! Player data: box scores date by date and the daily injury report, from the player source.
//!
//! A backfill walks the dates of a season that have a played game and no box-score fetch, so
//! a run that stops (a rate limit, a restart) resumes where it stopped. The daily sync is the
//! same walk over the current season, a second look at the last few days, and today's
//! injury report. `PlayerPhase` is the state the Data page polls while a run is on.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use chrono::{DateTime, Duration, NaiveDate, Utc};
use serde::Serialize;
use tracing::{error, info};

use crate::domain::ids::SeasonYear;

use super::ports::{FetchProgress, PlayerSource, PlayerStore, SourceError, StoreError};
use super::results_sync::Trigger;
use super::state::App;

/// The daily sync fetches again the dates of the last this many days, so lines stored
/// while a game was still on are replaced by the final ones.
pub const SETTLING_DAYS: i64 = 2;

/// What one run of the player sync stored.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct PlayerSyncOutcome {
    pub dates_fetched: usize,
    pub lines: i64,
    pub matched: i64,
    pub unmatched: i64,
    pub players_upserted: i64,
    /// The injury report stored, when the run fetched one: its date and its line count.
    pub injury_report: Option<(NaiveDate, i64)>,
    /// Players written from the source's whole player list, when the run loaded it.
    pub players_loaded: Option<i64>,
    /// Players on a current roster, when the run refreshed the rosters.
    pub active_players: Option<i64>,
}

/// Progress of a run: the dates done and the fetch under way.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PlayerProgress {
    pub dates_total: usize,
    pub dates_done: usize,
    pub current_date: Option<NaiveDate>,
    pub lines: i64,
    pub fetch: FetchProgress,
}

#[derive(Debug, thiserror::Error)]
pub enum PlayerSyncError {
    #[error(transparent)]
    Source(#[from] SourceError),
    #[error(transparent)]
    Store(#[from] StoreError),
}

/// A run that stopped before its end. What it stored stays stored; the next run resumes
/// with the dates left.
#[derive(Debug)]
pub struct Interrupted {
    pub done: PlayerSyncOutcome,
    pub dates_total: usize,
    pub error: PlayerSyncError,
}

impl std::fmt::Display for Interrupted {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.error)?;
        if self.dates_total == 0 {
            Ok(())
        } else if self.done.dates_fetched < self.dates_total {
            write!(
                f,
                "; {} of {} dates stored, the next run resumes with the rest",
                self.done.dates_fetched, self.dates_total
            )
        } else {
            write!(f, "; all {} dates stored", self.dates_total)
        }
    }
}

impl std::error::Error for Interrupted {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.error)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PlayerSyncKind {
    /// Box scores of every unfetched date of the season.
    Backfill { season: SeasonYear },
    /// The current season's unfetched and settling dates, today's injury report, then
    /// the current rosters.
    Daily { season: SeasonYear },
    /// Today's injury report only.
    InjuryReport,
    /// The source's whole player list.
    AllPlayers,
    /// The current rosters only.
    ActivePlayers,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "phase", rename_all = "snake_case")]
pub enum PlayerPhase {
    Idle,
    Fetching {
        kind: PlayerSyncKind,
        trigger: Trigger,
        started_at: DateTime<Utc>,
        dates_total: usize,
        dates_done: usize,
        current_date: Option<NaiveDate>,
        lines: i64,
        requests: u32,
        waiting_secs: Option<u64>,
        rate_limited: bool,
    },
    Done {
        kind: PlayerSyncKind,
        trigger: Trigger,
        started_at: DateTime<Utc>,
        finished_at: DateTime<Utc>,
        outcome: PlayerSyncOutcome,
    },
    Failed {
        kind: PlayerSyncKind,
        trigger: Trigger,
        started_at: DateTime<Utc>,
        finished_at: DateTime<Utc>,
        error: String,
        /// What the run stored before it failed.
        outcome: PlayerSyncOutcome,
    },
}

impl PlayerPhase {
    pub fn is_running(&self) -> bool {
        matches!(self, PlayerPhase::Fetching { .. })
    }

    fn with_progress(&self, p: PlayerProgress) -> PlayerPhase {
        match self {
            PlayerPhase::Fetching {
                kind,
                trigger,
                started_at,
                ..
            } => PlayerPhase::Fetching {
                kind: *kind,
                trigger: *trigger,
                started_at: *started_at,
                dates_total: p.dates_total,
                dates_done: p.dates_done,
                current_date: p.current_date,
                lines: p.lines,
                requests: p.fetch.requests,
                waiting_secs: p.fetch.waiting.map(|d| d.as_secs()),
                rate_limited: p.fetch.rate_limited,
            },
            other => other.clone(),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum StartError {
    #[error("player sync is not configured: set BALLDONTLIE_API_KEY and restart")]
    NotConfigured,
    #[error("a player sync is already running")]
    AlreadyRunning,
}

/// Fetch and store the box scores of `dates`, in order, reporting progress after every
/// request. Stops at the first error; the dates done stay stored.
async fn fetch_dates<P: PlayerSource, S: PlayerStore>(
    store: &S,
    source: &P,
    dates: &[NaiveDate],
    progress: &(dyn Fn(PlayerProgress) + Sync),
) -> Result<PlayerSyncOutcome, Interrupted> {
    let mut done = PlayerSyncOutcome::default();
    let mut requests = 0u32;
    for (i, &date) in dates.iter().enumerate() {
        // The source's last count for this date, added to the total when the date is done.
        let last = AtomicU32::new(0);
        let sink = |p: FetchProgress| {
            last.store(p.requests, Ordering::Relaxed);
            progress(PlayerProgress {
                dates_total: dates.len(),
                dates_done: i,
                current_date: Some(date),
                lines: done.lines,
                fetch: FetchProgress {
                    requests: requests + p.requests,
                    ..p
                },
            })
        };
        let step = async {
            let lines = source.box_scores_on(date, &sink).await?;
            let counts = store.save_box_scores(date, source.name(), &lines).await?;
            Ok::<_, PlayerSyncError>(counts)
        };
        match step.await {
            Ok(counts) => {
                done.dates_fetched += 1;
                done.lines += counts.lines;
                done.matched += counts.matched;
                done.unmatched += counts.unmatched;
                done.players_upserted += counts.players_upserted;
                // A date with no lines still costs one request.
                requests += last.load(Ordering::Relaxed).max(1);
                if counts.unmatched > 0 {
                    info!(%date, unmatched = counts.unmatched, "box-score lines without a stored game or team");
                }
            }
            Err(error) => {
                return Err(Interrupted {
                    done,
                    dates_total: dates.len(),
                    error,
                });
            }
        }
    }
    progress(PlayerProgress {
        dates_total: dates.len(),
        dates_done: dates.len(),
        current_date: None,
        lines: done.lines,
        fetch: FetchProgress {
            requests,
            ..Default::default()
        },
    });
    Ok(done)
}

/// Box scores of every date of `season` that has a played game and no fetch yet.
pub async fn backfill_season<P: PlayerSource, S: PlayerStore>(
    store: &S,
    source: &P,
    season: SeasonYear,
    progress: &(dyn Fn(PlayerProgress) + Sync),
) -> Result<PlayerSyncOutcome, Interrupted> {
    let dates = store
        .dates_without_box_scores(season)
        .await
        .map_err(|e| Interrupted {
            done: PlayerSyncOutcome::default(),
            dates_total: 0,
            error: e.into(),
        })?;
    info!(%season, dates = dates.len(), "box-score backfill: dates to fetch");
    fetch_dates(store, source, &dates, progress).await
}

/// The dates the daily sync fetches: the unfetched dates, and the fetched dates of the
/// settling window (their lines may have been stored while a game was on), in order.
pub fn settling_dates(
    unfetched: Vec<NaiveDate>,
    fetched: impl IntoIterator<Item = NaiveDate>,
    today: NaiveDate,
) -> Vec<NaiveDate> {
    let mut dates = unfetched;
    let settling = today - Duration::days(SETTLING_DAYS);
    for date in fetched {
        if date >= settling && date <= today && !dates.contains(&date) {
            dates.push(date);
        }
    }
    dates.sort_unstable();
    dates.dedup();
    dates
}

async fn daily_dates<S: PlayerStore>(
    store: &S,
    season: SeasonYear,
    today: NaiveDate,
) -> Result<Vec<NaiveDate>, StoreError> {
    let unfetched = store.dates_without_box_scores(season).await?;
    let fetched = store.box_score_fetches(season).await?;
    Ok(settling_dates(
        unfetched,
        fetched.iter().map(|f| f.game_date),
        today,
    ))
}

/// Store today's injury report under `today`.
pub async fn fetch_injury_report<P: PlayerSource, S: PlayerStore>(
    store: &S,
    source: &P,
    today: NaiveDate,
    progress: &(dyn Fn(PlayerProgress) + Sync),
) -> Result<i64, PlayerSyncError> {
    let sink = |p: FetchProgress| {
        progress(PlayerProgress {
            current_date: Some(today),
            fetch: p,
            ..Default::default()
        })
    };
    let report = source.injury_report(&sink).await?;
    let stored = store
        .save_injury_report(today, source.name(), &report)
        .await?;
    info!(date = %today, lines = stored, "injury report stored");
    Ok(stored)
}

/// Load the source's whole player list; returns how many players were written.
pub async fn load_all_players<P: PlayerSource, S: PlayerStore>(
    store: &S,
    source: &P,
    progress: &(dyn Fn(PlayerProgress) + Sync),
) -> Result<i64, PlayerSyncError> {
    let sink = |p: FetchProgress| {
        progress(PlayerProgress {
            lines: p.games as i64,
            fetch: p,
            ..Default::default()
        })
    };
    let players = source.all_players(&sink).await?;
    let written = store.save_players(source.name(), &players).await?;
    info!(players = written, "player list stored");
    Ok(written)
}

/// Refresh the current rosters; returns the active count.
pub async fn refresh_active_players<P: PlayerSource, S: PlayerStore>(
    store: &S,
    source: &P,
    progress: &(dyn Fn(PlayerProgress) + Sync),
) -> Result<i64, PlayerSyncError> {
    let sink = |p: FetchProgress| {
        progress(PlayerProgress {
            lines: p.games as i64,
            fetch: p,
            ..Default::default()
        })
    };
    let players = source.active_players(&sink).await?;
    // An empty roster list is a gap at the source, not a league without players: it must
    // not mark every player inactive.
    if players.is_empty() {
        return Err(
            SourceError::InvalidPayload("the source listed no active players".to_string()).into(),
        );
    }
    let active = store.save_active_players(source.name(), &players).await?;
    info!(active, "rosters stored");
    Ok(active)
}

/// The daily run: the current season's box scores, the injury report, the rosters.
pub async fn sync_daily<P: PlayerSource, S: PlayerStore>(
    store: &S,
    source: &P,
    season: SeasonYear,
    today: NaiveDate,
    progress: &(dyn Fn(PlayerProgress) + Sync),
) -> Result<PlayerSyncOutcome, Interrupted> {
    let dates = daily_dates(store, season, today)
        .await
        .map_err(|e| Interrupted {
            done: PlayerSyncOutcome::default(),
            dates_total: 0,
            error: e.into(),
        })?;
    let mut done = fetch_dates(store, source, &dates, progress).await?;
    let interrupted = |done: PlayerSyncOutcome, error| Interrupted {
        dates_total: dates.len(),
        done,
        error,
    };
    match fetch_injury_report(store, source, today, progress).await {
        Ok(lines) => done.injury_report = Some((today, lines)),
        Err(error) => return Err(interrupted(done, error)),
    }
    match refresh_active_players(store, source, progress).await {
        Ok(active) => done.active_players = Some(active),
        Err(error) => return Err(interrupted(done, error)),
    }
    Ok(done)
}

/// Run one player sync while the caller holds the player lock, publishing progress to
/// `app.player_phase()`.
pub async fn run_tracked<P: PlayerSource>(
    app: &App,
    source: &P,
    kind: PlayerSyncKind,
    trigger: Trigger,
) -> Result<PlayerSyncOutcome, Interrupted> {
    let started_at = Utc::now();
    app.set_player_phase(PlayerPhase::Fetching {
        kind,
        trigger,
        started_at,
        dates_total: 0,
        dates_done: 0,
        current_date: None,
        lines: 0,
        requests: 0,
        waiting_secs: None,
        rate_limited: false,
    });
    let progress = |p: PlayerProgress| {
        let current = app.player_phase();
        app.set_player_phase(current.with_progress(p));
    };
    let today = app.today();
    // A run of one fetch has no dates to resume.
    let alone = |error| Interrupted {
        done: PlayerSyncOutcome::default(),
        dates_total: 0,
        error,
    };
    let result = match kind {
        PlayerSyncKind::Backfill { season } => {
            backfill_season(&app.store, source, season, &progress).await
        }
        PlayerSyncKind::Daily { season } => {
            sync_daily(&app.store, source, season, today, &progress).await
        }
        PlayerSyncKind::InjuryReport => fetch_injury_report(&app.store, source, today, &progress)
            .await
            .map(|lines| PlayerSyncOutcome {
                injury_report: Some((today, lines)),
                ..Default::default()
            })
            .map_err(alone),
        PlayerSyncKind::AllPlayers => load_all_players(&app.store, source, &progress)
            .await
            .map(|n| PlayerSyncOutcome {
                players_loaded: Some(n),
                ..Default::default()
            })
            .map_err(alone),
        PlayerSyncKind::ActivePlayers => refresh_active_players(&app.store, source, &progress)
            .await
            .map(|n| PlayerSyncOutcome {
                active_players: Some(n),
                ..Default::default()
            })
            .map_err(alone),
    };
    let finished_at = Utc::now();
    match &result {
        Ok(outcome) => {
            info!(
                ?kind,
                ?trigger,
                dates = outcome.dates_fetched,
                lines = outcome.lines,
                matched = outcome.matched,
                unmatched = outcome.unmatched,
                "player sync completed"
            );
            app.set_player_phase(PlayerPhase::Done {
                kind,
                trigger,
                started_at,
                finished_at,
                outcome: outcome.clone(),
            });
        }
        Err(e) => {
            error!(?kind, ?trigger, error = %e, "player sync failed");
            app.set_player_phase(PlayerPhase::Failed {
                kind,
                trigger,
                started_at,
                finished_at,
                error: e.to_string(),
                outcome: e.done.clone(),
            });
        }
    }
    result
}

/// Start a manual run in the background. Returns at once; poll `app.player_phase()`.
pub fn start(app: Arc<App>, kind: PlayerSyncKind) -> Result<(), StartError> {
    if app.results.is_none() {
        return Err(StartError::NotConfigured);
    }
    let guard = app
        .player_lock
        .clone()
        .try_lock_owned()
        .map_err(|_| StartError::AlreadyRunning)?;
    tokio::spawn(async move {
        let _guard = guard;
        if let Some(source) = app.results.as_ref() {
            let _ = run_tracked(&app, source, kind, Trigger::Manual).await;
        }
    });
    Ok(())
}

/// The scheduled daily run for `season`. Skipped when a manual run holds the lock.
pub async fn sync_scheduled<P: PlayerSource>(app: &App, source: &P, season: SeasonYear) {
    let Ok(_guard) = app.player_lock.clone().try_lock_owned() else {
        info!("a manual player sync is running; the scheduled player sync waits for the next run");
        return;
    };
    let _ = run_tracked(
        app,
        source,
        PlayerSyncKind::Daily { season },
        Trigger::Scheduled,
    )
    .await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration as StdDuration;

    #[test]
    fn progress_only_updates_a_running_phase() {
        let t = Utc::now();
        let kind = PlayerSyncKind::Backfill {
            season: SeasonYear::of(2026),
        };
        let running = PlayerPhase::Fetching {
            kind,
            trigger: Trigger::Manual,
            started_at: t,
            dates_total: 0,
            dates_done: 0,
            current_date: None,
            lines: 0,
            requests: 0,
            waiting_secs: None,
            rate_limited: false,
        };
        let date = NaiveDate::from_ymd_opt(2025, 11, 3).unwrap();
        let p = PlayerProgress {
            dates_total: 160,
            dates_done: 12,
            current_date: Some(date),
            lines: 3_000,
            fetch: FetchProgress {
                requests: 40,
                games: 150,
                waiting: Some(StdDuration::from_secs(11)),
                rate_limited: true,
            },
        };
        assert_eq!(
            running.with_progress(p),
            PlayerPhase::Fetching {
                kind,
                trigger: Trigger::Manual,
                started_at: t,
                dates_total: 160,
                dates_done: 12,
                current_date: Some(date),
                lines: 3_000,
                requests: 40,
                waiting_secs: Some(11),
                rate_limited: true,
            }
        );
        assert_eq!(PlayerPhase::Idle.with_progress(p), PlayerPhase::Idle);
    }

    #[test]
    fn the_daily_run_takes_the_unfetched_dates_and_the_settling_window() {
        let d = |day: u32| NaiveDate::from_ymd_opt(2026, 1, day).unwrap();
        let today = d(20);
        let dates = settling_dates(
            vec![d(19), d(5)],
            [d(17), d(18), d(19), d(20), d(21), d(1)],
            today,
        );
        // The 18th to the 20th are within 2 days of today; the 17th and a future date are
        // not; the 19th is listed once.
        assert_eq!(dates, vec![d(5), d(18), d(19), d(20)]);
        assert!(settling_dates(vec![], [], today).is_empty());
    }

    #[test]
    fn interrupted_names_the_dates_left() {
        let e = Interrupted {
            done: PlayerSyncOutcome {
                dates_fetched: 3,
                ..Default::default()
            },
            dates_total: 10,
            error: SourceError::RateLimited(5).into(),
        };
        let text = e.to_string();
        assert!(text.contains("3 of 10 dates stored"), "{text}");
        assert!(text.contains("rate limit"), "{text}");
        let all = Interrupted {
            done: PlayerSyncOutcome {
                dates_fetched: 10,
                ..Default::default()
            },
            dates_total: 10,
            error: SourceError::Transport("timed out".into()).into(),
        };
        assert!(all.to_string().ends_with("all 10 dates stored"), "{all}");
        let none = Interrupted {
            done: PlayerSyncOutcome::default(),
            dates_total: 0,
            error: SourceError::Transport("timed out".into()).into(),
        };
        assert!(none.to_string().ends_with("timed out"), "{none}");
    }
}
