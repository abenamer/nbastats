//! Daily results sync: once a day, at a configured local time, pull the current season from
//! the results source and recompute its Elo. The schedule is pure (`next_run`,
//! `current_season`); the loop is a background task that follows the application's
//! shutdown token like the job runner.

use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Datelike, NaiveTime, TimeZone, Utc};
use chrono_tz::Tz;
use tracing::{error, info, warn};

use crate::domain::ids::SeasonYear;
use crate::domain::models::ImportStatus;

use super::continuation::{self, ContinuationUseCaseError};
use super::ports::{ImportLog, ResultsSource};
use super::state::App;

/// When the daily sync runs, as a wall-clock time in a named time zone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SyncSchedule {
    pub time: NaiveTime,
    pub zone: Tz,
}

#[derive(Debug, thiserror::Error)]
pub enum ScheduleError {
    #[error("invalid sync time {0:?}; use HH:MM")]
    Time(String),
    #[error("unknown time zone {0:?}; use an IANA name such as America/New_York")]
    Zone(String),
}

impl SyncSchedule {
    pub fn parse(time: &str, zone: &str) -> Result<Self, ScheduleError> {
        let time = NaiveTime::parse_from_str(time.trim(), "%H:%M")
            .map_err(|_| ScheduleError::Time(time.to_string()))?;
        let zone: Tz = zone
            .trim()
            .parse()
            .map_err(|_| ScheduleError::Zone(zone.to_string()))?;
        Ok(Self { time, zone })
    }

    /// The first scheduled instant strictly after `after`.
    pub fn next_run(&self, after: DateTime<Utc>) -> DateTime<Utc> {
        let local = after.with_timezone(&self.zone);
        let mut day = local.date_naive();
        loop {
            // Around a daylight-saving change a wall-clock time may be missing (skip to the
            // next valid instant) or doubled (take the earlier one).
            let candidate = self
                .zone
                .from_local_datetime(&day.and_time(self.time))
                .earliest()
                .or_else(|| {
                    self.zone
                        .from_local_datetime(&day.and_time(self.time + chrono::Duration::hours(1)))
                        .earliest()
                });
            if let Some(c) = candidate {
                let utc = c.with_timezone(&Utc);
                if utc > after {
                    return utc;
                }
            }
            day = day.succ_opt().expect("date range");
        }
    }

    /// The most recent scheduled instant at or before `now`.
    pub fn previous_run(&self, now: DateTime<Utc>) -> DateTime<Utc> {
        // The run before `next_run(now)` is at most 25 hours earlier; step back day by day.
        let next = self.next_run(now);
        let mut probe = next - chrono::Duration::hours(23);
        loop {
            let candidate = self.next_run(probe);
            if candidate <= now {
                return candidate;
            }
            probe -= chrono::Duration::hours(1);
        }
    }
}

/// The season whose games are being played (or about to be): September to December belong
/// to the season that ends next year; January to August to the season that ends this year.
pub fn current_season(today: chrono::NaiveDate) -> SeasonYear {
    let end_year = if today.month() >= 9 {
        today.year() + 1
    } else {
        today.year()
    };
    SeasonYear::try_new(end_year).unwrap_or(SeasonYear::MAX)
}

/// Sync the current season once. When the previous season is missing (for example the
/// first run after a new season started), that season is synced first so the ratings
/// chain from the archive. Imported seasons are skipped, not errors.
pub async fn sync_current<R: ResultsSource>(app: &App, source: &R) -> Option<SeasonYear> {
    let season = current_season(
        Utc::now()
            .with_timezone(&app.sync_schedule?.zone)
            .date_naive(),
    );
    // At most two seasons back: the archive ends one season before the first computed one.
    let mut pending = vec![season];
    while let Some(target) = pending.pop() {
        match continuation::sync_results(&app.store, source, target).await {
            Ok(outcome) => {
                app.invalidate_cache();
                info!(
                    season = %target,
                    inserted = outcome.games_inserted,
                    updated = outcome.games_updated,
                    unchanged = outcome.games_unchanged,
                    "scheduled results sync completed"
                );
                if target == season {
                    return Some(season);
                }
            }
            Err(ContinuationUseCaseError::NoPreviousSeason(previous)) if pending.len() < 2 => {
                info!(season = %target, %previous, "previous season missing; syncing it first");
                pending.push(target);
                pending.push(previous);
            }
            Err(ContinuationUseCaseError::SeasonHasImportedElo(s)) => {
                info!(season = %s, "scheduled results sync skipped: season has imported Elo");
                return None;
            }
            Err(e) => {
                error!(season = %target, error = %e, "scheduled results sync failed");
                return None;
            }
        }
    }
    None
}

/// Whether a completed sync from `source` has run since the last scheduled instant.
async fn synced_since<S: ImportLog>(
    store: &S,
    source: &str,
    since: DateTime<Utc>,
) -> Result<bool, super::ports::StoreError> {
    Ok(store.imports(50).await?.iter().any(|i| {
        i.source == source && i.status == ImportStatus::Completed && i.started_at >= since
    }))
}

/// Background task: catch up at startup when the last scheduled run was missed, then run
/// at every scheduled instant until shutdown.
pub async fn run(app: Arc<App>) {
    let (Some(source), Some(schedule)) = (app.results.as_ref(), app.sync_schedule) else {
        return;
    };
    info!(time = %schedule.time, zone = %schedule.zone, "daily results sync scheduled");
    // Let the server finish starting before the first network call.
    tokio::select! {
        _ = tokio::time::sleep(Duration::from_secs(20)) => {}
        _ = app.shutdown.cancelled() => return,
    }
    let now = Utc::now();
    match synced_since(&app.store, source.name(), schedule.previous_run(now)).await {
        Ok(true) => {}
        Ok(false) => {
            info!("no results sync since the last scheduled time; syncing now");
            sync_current(&app, source).await;
        }
        Err(e) => warn!(error = %e, "could not read the import log; skipping catch-up"),
    }
    loop {
        let next = schedule.next_run(Utc::now());
        let wait = (next - Utc::now()).to_std().unwrap_or(Duration::ZERO);
        info!(next_run = %next, "next results sync");
        tokio::select! {
            _ = tokio::time::sleep(wait) => {}
            _ = app.shutdown.cancelled() => break,
        }
        sync_current(&app, source).await;
    }
    info!("results sync scheduler stopped");
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn utc(s: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
    }

    #[test]
    fn next_run_is_four_am_new_york_in_utc() {
        let s = SyncSchedule::parse("04:00", "America/New_York").unwrap();
        // EST (UTC-5): 04:00 local = 09:00 UTC
        assert_eq!(
            s.next_run(utc("2026-01-15T08:59:00Z")),
            utc("2026-01-15T09:00:00Z")
        );
        assert_eq!(
            s.next_run(utc("2026-01-15T09:00:00Z")),
            utc("2026-01-16T09:00:00Z")
        );
        // EDT (UTC-4): 04:00 local = 08:00 UTC
        assert_eq!(
            s.next_run(utc("2026-06-01T12:00:00Z")),
            utc("2026-06-02T08:00:00Z")
        );
        // The spring-forward night (2026-03-08) still has a 04:00; the run moves with the clock.
        assert_eq!(
            s.next_run(utc("2026-03-08T07:30:00Z")),
            utc("2026-03-08T08:00:00Z")
        );
    }

    #[test]
    fn previous_run_is_the_last_scheduled_instant() {
        let s = SyncSchedule::parse("04:00", "America/New_York").unwrap();
        assert_eq!(
            s.previous_run(utc("2026-01-15T10:00:00Z")),
            utc("2026-01-15T09:00:00Z")
        );
        assert_eq!(
            s.previous_run(utc("2026-01-15T08:00:00Z")),
            utc("2026-01-14T09:00:00Z")
        );
        assert_eq!(
            s.previous_run(utc("2026-01-15T09:00:00Z")),
            utc("2026-01-15T09:00:00Z")
        );
    }

    #[test]
    fn rejects_bad_schedule_values() {
        assert!(matches!(
            SyncSchedule::parse("4am", "America/New_York"),
            Err(ScheduleError::Time(_))
        ));
        assert!(matches!(
            SyncSchedule::parse("04:00", "EST5EDT/nowhere"),
            Err(ScheduleError::Zone(_))
        ));
    }

    #[test]
    fn current_season_follows_the_nba_calendar() {
        let d = |y, m, d| NaiveDate::from_ymd_opt(y, m, d).unwrap();
        assert_eq!(current_season(d(2025, 10, 21)), SeasonYear::of(2026));
        assert_eq!(current_season(d(2026, 3, 1)), SeasonYear::of(2026));
        assert_eq!(current_season(d(2026, 8, 31)), SeasonYear::of(2026));
        assert_eq!(current_season(d(2026, 9, 1)), SeasonYear::of(2027));
    }
}
