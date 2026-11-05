//! Title futures: once a day while the regular season runs, read the sportsbook's outright
//! prices on the title (The Odds API, one credit), forecast the season from today with the
//! injury report so the model has a title chance of the same day, and keep both per team.
//! Once the season is decided, the days are scored against the champion. The policy and the
//! scoring are `domain::futures`; the prices come through the `FuturesSource` port and the
//! rows live behind the `MarketStore` port.

use std::collections::HashMap;
use std::sync::PoisonError;
use std::time::Duration;

use chrono::{DateTime, NaiveDate, Utc};
use serde::Serialize;
use tracing::{info, warn};

use crate::application::ports::{FuturesSource, GameLog, MarketStore, SourceError, StoreError};
use crate::application::state::{App, SeasonLoadError};
use crate::application::worth as app_worth;
use crate::config::APP_VERSION;
use crate::domain::futures::{self, DayScore, FuturesSnapshot, Record};
use crate::domain::ids::SeasonYear;
use crate::domain::market;
use crate::domain::playoffs::reconstruct;
use crate::domain::playoffs::season::{self as engine, SeasonParams};
use crate::domain::playoffs::SEASON_MODEL_VERSION;
use crate::domain::simulation::progress::Silent;
use crate::domain::tracking::tonight_model_version;

/// Simulated seasons of the day's forecast: as the question box's comparison, enough for
/// a title chance with about one point of simulation noise, a few seconds a day.
pub const FORECAST_ITERATIONS: u64 = 2000;
pub const FORECAST_SEED: u64 = 7;
/// After a failed read or forecast, the next try waits this long.
pub const RETRY_AFTER: Duration = Duration::from_secs(60 * 60);

/// What the daily capture remembers: the day it has captured (or found nothing to
/// capture), and when to try again after a failure.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Progress {
    pub day: Option<NaiveDate>,
    pub retry_at: Option<DateTime<Utc>>,
}

impl Progress {
    /// Whether a capture is due at `now` for `today`: a day not yet captured, or a retry
    /// that has come.
    pub fn due(&self, today: NaiveDate, now: DateTime<Utc>) -> bool {
        self.day != Some(today) || self.retry_at.is_some_and(|at| now >= at)
    }

    /// The day is done, captured or with nothing to capture.
    pub fn done(&mut self, today: NaiveDate) {
        self.day = Some(today);
        self.retry_at = None;
    }

    /// The day's capture failed: try again after [`RETRY_AFTER`].
    pub fn failed(&mut self, today: NaiveDate, now: DateTime<Utc>) {
        self.day = Some(today);
        self.retry_at = Some(now + chrono::Duration::from_std(RETRY_AFTER).unwrap_or_default());
    }
}

/// The model version a futures row carries: the season engine's, and the parts that price
/// the absences and the full-strength rating it starts from.
pub fn model_version() -> String {
    format!("{SEASON_MODEL_VERSION} {}", tonight_model_version())
}

#[derive(Debug, thiserror::Error)]
pub enum CaptureError {
    #[error(transparent)]
    Season(#[from] SeasonLoadError),
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error("the futures could not be read: {0}")]
    Source(#[from] SourceError),
    #[error("the forecast failed: {0}")]
    Forecast(String),
    #[error("no team of the season matches the sportsbook's names")]
    NoTeams,
}

/// Whether `today` is a day to capture for `season`: the season has games, and its last
/// regular-season game is today or later. The playoffs are not captured: the season
/// engine forecasts from before the playoffs, so a title chance during them would not be
/// the model's chance on the day.
pub fn capture_day(games: &[crate::domain::models::Game], today: NaiveDate) -> bool {
    engine::last_regular_game(games).is_some_and(|last| today <= last)
}

/// The model's title chance per team code from a forecast as of `today` with the injury
/// report, the full-strength rating and the fixed penalties, as the question box's
/// comparison runs it.
pub async fn model_chances(
    app: &App,
    season: SeasonYear,
    today: NaiveDate,
) -> Result<HashMap<String, (f64, f64)>, CaptureError> {
    let data = app.season_data(season).await?;
    let a = app_worth::availability(app, season).await?;
    let home = app.home_advantage(season).await?;
    let teams = app.teams().await?;
    let mut params = SeasonParams {
        hold_absences: false,
        rating_shifts: app_worth::rating_shifts(app, season, today).await?,
        worth_overrides: Vec::new(),
        season,
        as_of: Some(today),
        iterations: FORECAST_ITERATIONS,
        seed: FORECAST_SEED,
        home_advantage: 100.0,
        regular_home_advantage: Some(home.regular),
        regular_season_k: Some(20.0),
        playoff_k: None,
        playoff_scale: 1.0,
        absences: a.absences,
        rating_overrides: Vec::new(),
        excluded_teams: Vec::new(),
        qualification: None,
        first_round_games: None,
    };
    app_worth::fix_penalties(&mut params.absences);
    let games = data.games.clone();
    let forecast = tokio::task::spawn_blocking(move || {
        engine::run(&params, &games, &teams, &Silent).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| CaptureError::Forecast(e.to_string()))?
    .map_err(CaptureError::Forecast)?;
    Ok(forecast
        .teams
        .into_iter()
        .map(|t| (t.code, (t.p_champion, t.p_champion_se)))
        .collect())
}

/// Read the sportsbook's field and the model's chances of `today` and keep them, once a
/// day. Returns the rows written: 0 when nothing was due or there is nothing to capture.
pub async fn capture<S: FuturesSource>(
    app: &App,
    source: &S,
    season: SeasonYear,
) -> Result<u64, CaptureError> {
    let today = app.today();
    let now = Utc::now();
    let due = app
        .futures
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .due(today, now);
    if !due {
        return Ok(0);
    }
    let outcome = capture_today(app, source, season, today, now).await;
    let mut progress = app.futures.lock().unwrap_or_else(PoisonError::into_inner);
    match outcome {
        Ok(n) => {
            progress.done(today);
            Ok(n)
        }
        // A season with no games, or past its regular season, is nothing to capture today.
        Err(e @ CaptureError::Season(SeasonLoadError::NoGames(_))) => {
            progress.done(today);
            Err(e)
        }
        // An answer the application cannot keep (the sportsbook prices no title or names no
        // team of the season, or an answer that is not an outright listing) will not
        // change within the day: the next try is tomorrow's, so a configuration that
        // cannot work costs one credit a day, not one an hour.
        Err(e @ (CaptureError::NoTeams | CaptureError::Source(SourceError::InvalidPayload(_)))) => {
            progress.done(today);
            Err(e)
        }
        // A failure that may pass (the service unreachable, a store error, a forecast that
        // failed): try again after the cooldown.
        Err(e) => {
            progress.failed(today, now);
            Err(e)
        }
    }
}

/// Returns `Ok(0)` when `today` is not a day to capture.
async fn capture_today<S: FuturesSource>(
    app: &App,
    source: &S,
    season: SeasonYear,
    today: NaiveDate,
    now: DateTime<Utc>,
) -> Result<u64, CaptureError> {
    let data = app.season_data(season).await?;
    if !capture_day(&data.games, today) {
        return Ok(0);
    }
    // Everything the application can do alone comes before the paid read, so a failure
    // of its own costs no credit: the season's teams, their names, and the forecast.
    let teams = app.teams().await?;
    let season_codes: std::collections::HashSet<String> = data
        .games
        .iter()
        .flat_map(|g| [g.home_team_id, g.away_team_id])
        .filter_map(|id| teams.get(&id).map(|t| t.code.clone()))
        .collect();
    let by_name: HashMap<String, String> = app
        .store
        .team_code_map()
        .await?
        .into_iter()
        .filter(|(code, _)| season_codes.contains(code))
        .map(|(code, team)| (team.name.trim().to_ascii_lowercase(), code))
        .collect();
    let model = model_chances(app, season, today).await?;
    let (provider, prices) = source.title_futures().await?;
    // The field is the season's teams the sportsbook names; a name it does not match (a
    // team of another era, a renamed one, or the field's "any other") is left out and
    // logged.
    let mut field: Vec<(String, f64)> = Vec::new();
    let mut unknown = 0;
    for p in &prices {
        match by_name.get(&p.name.trim().to_ascii_lowercase()) {
            Some(code) => field.push((code.clone(), p.price)),
            None => unknown += 1,
        }
    }
    if unknown > 0 {
        warn!(
            unknown,
            listed = prices.len(),
            "title futures name teams the store does not know"
        );
    }
    let fair = market::fair_field(&field.iter().map(|(_, p)| *p).collect::<Vec<_>>())
        .ok_or(CaptureError::NoTeams)?;
    let version = model_version();
    let rows: Vec<FuturesSnapshot> = field
        .iter()
        .zip(fair.chances.iter())
        .map(|((code, price), p_market)| {
            let (p_model, p_model_se) = model.get(code).copied().unwrap_or((0.0, 0.0));
            FuturesSnapshot {
                season,
                snapshot_date: today,
                team_code: code.clone(),
                captured_at: now,
                provider: provider.clone(),
                source: source.name().to_string(),
                price: *price,
                p_market: *p_market,
                book_margin: fair.margin,
                p_model,
                p_model_se,
                model_version: version.clone(),
                app_version: APP_VERSION.to_string(),
            }
        })
        .collect();
    let written = app.store.save_futures(&rows).await?;
    info!(
        %season,
        day = %today,
        teams = rows.len(),
        margin = format!("{:.3}", fair.margin),
        "title futures captured"
    );
    Ok(written)
}

/// One team's row of the latest day's field, for the page.
#[derive(Debug, Clone, Serialize)]
pub struct FuturesRow {
    pub code: String,
    pub name: String,
    pub p_model: f64,
    pub p_model_se: f64,
    pub price: f64,
    pub p_market: f64,
    /// The model's chance less the sportsbook's.
    pub gap: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct LatestField {
    pub date: NaiveDate,
    pub captured_at: DateTime<Utc>,
    pub provider: String,
    pub source: String,
    pub book_margin: f64,
    pub model_version: String,
    pub rows: Vec<FuturesRow>,
}

/// A season's title futures as the page shows them: the latest day's field, the days
/// saved, and the record against the champion once the season is decided.
#[derive(Debug, Clone, Serialize)]
pub struct FuturesReport {
    pub season: SeasonYear,
    pub days: usize,
    pub first_day: Option<NaiveDate>,
    pub last_day: Option<NaiveDate>,
    pub latest: Option<LatestField>,
    /// The champion's code once the season is decided.
    pub champion: Option<String>,
    pub record: Option<Record>,
    /// Each scored day, earliest first, for the chart.
    pub scored_days: Vec<DayScore>,
    pub current_model: String,
    pub source_configured: bool,
    pub note: Option<String>,
}

pub async fn report(app: &App, season: SeasonYear) -> Result<FuturesReport, SeasonLoadError> {
    let snapshots = app.store.futures(season).await?;
    let teams = app.teams().await?;
    let names: HashMap<&str, &str> = teams
        .values()
        .map(|t| (t.code.as_str(), t.name.as_str()))
        .collect();
    let days = futures::days_of(&snapshots);
    let latest = futures::latest_day(&snapshots).map(|(date, rows)| LatestField {
        date,
        captured_at: rows.iter().map(|r| r.captured_at).max().unwrap_or_default(),
        provider: rows.first().map(|r| r.provider.clone()).unwrap_or_default(),
        source: rows.first().map(|r| r.source.clone()).unwrap_or_default(),
        book_margin: rows.first().map(|r| r.book_margin).unwrap_or_default(),
        model_version: rows
            .first()
            .map(|r| r.model_version.clone())
            .unwrap_or_default(),
        rows: rows
            .iter()
            .map(|r| FuturesRow {
                code: r.team_code.clone(),
                name: names
                    .get(r.team_code.as_str())
                    .map(|n| n.to_string())
                    .unwrap_or_default(),
                p_model: r.p_model,
                p_model_se: r.p_model_se,
                price: r.price,
                p_market: r.p_market,
                gap: r.p_model - r.p_market,
            })
            .collect(),
    });
    // The champion: the decided postseason's, by the stored games.
    let champion = match app.season_data(season).await {
        Ok(data) => data
            .reconstruction
            .as_ref()
            .ok()
            .filter(|r| r.complete)
            .and_then(|r| reconstruct::champion(&r.series))
            .and_then(|(winner, _)| teams.get(&winner).map(|t| t.code.clone())),
        Err(SeasonLoadError::NoGames(_)) => None,
        Err(e) => return Err(e),
    };
    let scored_days = champion
        .as_deref()
        .map(|c| futures::score_days(&snapshots, c))
        .unwrap_or_default();
    let record = (!scored_days.is_empty()).then(|| futures::summarize(&scored_days));
    let source_configured = app.odds.is_some();
    let note = if snapshots.is_empty() {
        Some(if source_configured {
            "No title futures saved for this season yet: the capture runs once a day while the regular season runs, with market tracking on.".to_string()
        } else {
            "Title futures need The Odds API: set ODDS_API_KEY.".to_string()
        })
    } else if champion.is_none() {
        Some("The season is not decided: the days are scored against the champion once the postseason is complete.".to_string())
    } else if scored_days.is_empty() {
        Some("The champion is known, but no saved field names the champion, so no day can be scored.".to_string())
    } else {
        None
    };
    Ok(FuturesReport {
        season,
        days: days.len(),
        first_day: days.first().copied(),
        last_day: days.last().copied(),
        latest,
        champion,
        record,
        scored_days,
        current_model: model_version(),
        source_configured,
        note,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ids::TeamId;
    use crate::domain::models::{EloSource, Game, GameType, ResultSource};
    use chrono::TimeZone;

    fn at(d: u32, h: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 11, d, h, 0, 0).unwrap()
    }

    fn day(d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 11, d).unwrap()
    }

    #[test]
    fn a_capture_is_due_once_a_day_and_again_after_a_failure_cools_down() {
        let mut p = Progress::default();
        assert!(p.due(day(5), at(5, 10)));
        p.done(day(5));
        assert!(!p.due(day(5), at(5, 23)));
        assert!(p.due(day(6), at(6, 0)), "a new day");
        p.failed(day(6), at(6, 0));
        assert!(!p.due(day(6), at(6, 0)));
        assert!(p.due(day(6), at(6, 1)), "an hour later");
        p.done(day(6));
        assert!(!p.due(day(6), at(6, 2)));
    }

    #[test]
    fn the_capture_days_run_to_the_last_regular_season_game() {
        let game = |d: u32, t: GameType| Game {
            id: i64::from(d),
            season: SeasonYear::of(2027),
            game_date: day(d),
            seq: 0,
            home_team_id: TeamId::new(1),
            away_team_id: TeamId::new(2),
            neutral: false,
            game_type: t,
            home_score: None,
            away_score: None,
            home_elo_pre: 1500.0,
            away_elo_pre: 1500.0,
            home_elo_post: 1500.0,
            away_elo_post: 1500.0,
            home_win_prob: None,
            elo_source: EloSource::Imported,
            result_source: ResultSource::Archive,
            cup_stage: None,
            ref_home_elo_pre: None,
            ref_away_elo_pre: None,
            ref_home_elo_post: None,
            ref_away_elo_post: None,
        };
        let games = vec![
            game(1, GameType::Regular),
            game(20, GameType::Regular),
            game(28, GameType::Playoff),
        ];
        assert!(capture_day(&games, day(1)));
        assert!(
            capture_day(&games, day(20)),
            "the last regular-season day counts"
        );
        assert!(
            !capture_day(&games, day(21)),
            "the playoffs are not captured"
        );
        assert!(!capture_day(&[], day(1)));
        assert!(model_version().starts_with(SEASON_MODEL_VERSION));
    }
}
