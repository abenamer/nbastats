//! The model against the market, kept and scored. A background task captures, every
//! [`CAPTURE_EVERY`], the Tonight panel's games that the market source prices and that have
//! not started: the model's numbers and the sportsbook's lines, with the model's version
//! (`domain::tracking`). The last capture before a game's start stands for it. The track
//! record scores each stored game once its result is in, per model version and per month.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use chrono::{Datelike, NaiveDate};
use serde::Serialize;
use tracing::{info, warn};

use crate::domain::ids::{SeasonYear, TeamId};
use crate::domain::tracking::{self, Group, Outcome, Scored, Snapshot, Summary};

use super::market::{self, Market};
use super::ports::{GameLog, MarketStore, ReportSource};
use super::scheduler::current_season;
use super::state::{App, SeasonLoadError};
use super::worth::{self, Tonight};

/// How often the night's lines are captured; the last capture before a game's start is
/// at most this old, plus the lines' five minutes in the cache.
pub const CAPTURE_EVERY: Duration = Duration::from_secs(10 * 60);
/// The most scored games the record lists, latest first.
pub const MAX_GAMES_LISTED: usize = 300;

/// The snapshots of the night of `t` from its market `m`: each game the market prices
/// whose line was read before its start and while the source still listed it as not
/// started. The lines' read time is the capture's.
pub fn build_snapshots(
    season: SeasonYear,
    t: &Tonight,
    m: &Market,
    source: &str,
    model_version: &str,
) -> Vec<Snapshot> {
    let (Some(night), Some(captured_at)) = (t.night, m.read_at) else {
        return Vec::new();
    };
    let stored: HashMap<TeamId, f64> = t
        .teams
        .iter()
        .map(|x| (x.team_id, x.stored_rating))
        .collect();
    let lines: HashMap<i64, &market::GameMarket> = m.games.iter().map(|g| (g.game_id, g)).collect();
    t.games
        .iter()
        .filter_map(|g| {
            let line = lines.get(&g.game.game_id)?;
            // A line read after the start is not the one the game was played at.
            if line.started || line.start_time.is_some_and(|s| s <= captured_at) {
                return None;
            }
            let n = &g.game;
            Some(Snapshot {
                season,
                game_date: night,
                home_code: g.home_code.clone(),
                away_code: g.away_code.clone(),
                captured_at,
                start_time: line.start_time,
                model_version: model_version.to_string(),
                app_version: env!("CARGO_PKG_VERSION").to_string(),
                home_advantage: t.home_advantage,
                home_rating: n.home_rating,
                away_rating: n.away_rating,
                home_stored: stored
                    .get(&n.home_team_id)
                    .copied()
                    .unwrap_or(n.home_rating),
                away_stored: stored
                    .get(&n.away_team_id)
                    .copied()
                    .unwrap_or(n.away_rating),
                home_penalty: n.home_penalty,
                away_penalty: n.away_penalty,
                p_home_full: n.p_home_full,
                p_home_model: n.p_home_tonight,
                home_margin_model: n.home_margin_tonight,
                provider: line.provider.clone(),
                source: source.to_string(),
                home_spread: line.home_spread,
                home_spread_odds: line.home_spread_odds,
                away_spread_odds: line.away_spread_odds,
                home_moneyline: line.home_moneyline,
                away_moneyline: line.away_moneyline,
                total: line.total,
                p_home_market: line.p_home,
                book_margin: line.book_margin,
            })
        })
        .collect()
}

/// Capture the night's games of `season` that the market prices and that have not
/// started. Returns how many snapshots were written.
pub async fn capture(app: &App, season: SeasonYear) -> Result<u64, SeasonLoadError> {
    let Some(source) = app.reports.as_ref() else {
        return Ok(0);
    };
    let t = worth::tonight(app, season, None).await?;
    if t.night.is_none() {
        return Ok(0);
    }
    let m = market::market_for(app, season, &t).await?;
    let snapshots = build_snapshots(
        season,
        &t,
        &m,
        source.name(),
        &tracking::tonight_model_version(),
    );
    if snapshots.is_empty() {
        return Ok(0);
    }
    Ok(app.store.save_snapshots(&snapshots).await?)
}

/// Background task: capture the current season's night every [`CAPTURE_EVERY`] until
/// shutdown. Off without the ESPN feed or with `MARKET_TRACKING_ENABLED=false`.
pub async fn run(app: Arc<App>) {
    if app.reports.is_none() || !app.config.market_tracking_enabled {
        return;
    }
    info!(
        every_minutes = CAPTURE_EVERY.as_secs() / 60,
        "market tracking on"
    );
    // Let the server finish starting before the first network call.
    tokio::select! {
        _ = tokio::time::sleep(Duration::from_secs(30)) => {}
        _ = app.shutdown.cancelled() => return,
    }
    loop {
        let season = current_season(app.today());
        match capture(&app, season).await {
            Ok(0) => {}
            Ok(n) => info!(%season, written = n, "market snapshots captured"),
            Err(SeasonLoadError::NoGames(_)) => {}
            Err(e) => warn!(%season, error = %e, "market snapshots not captured"),
        }
        tokio::select! {
            _ = tokio::time::sleep(CAPTURE_EVERY) => {}
            _ = app.shutdown.cancelled() => break,
        }
    }
    info!("market tracking stopped");
}

/// A season's record of the model against the market.
#[derive(Debug, Clone, Serialize)]
pub struct TrackRecord {
    pub season: SeasonYear,
    /// Snapshots stored; those scored (the game played); those whose game is still to be
    /// played; and those that match no stored game (a game moved to another date) or whose
    /// stored score is a tie. The last three add up to the first.
    pub snapshots: usize,
    pub scored: usize,
    pub pending: usize,
    pub unmatched: usize,
    pub overall: Summary,
    pub by_model: Vec<Group>,
    pub by_month: Vec<Group>,
    /// The scored games, latest first, at most [`MAX_GAMES_LISTED`].
    pub games: Vec<Scored>,
    /// The model version capturing now, so a reader can tell the current row.
    pub current_model: String,
    pub note: Option<String>,
}

/// A stored game by date and team codes: its result once played.
pub type GameKey = (NaiveDate, String, String);

/// The snapshots matched to their games: scored when the game is played, pending while
/// it is not, unmatched when no stored game has the snapshot's date and teams or the
/// stored score is a tie. The three counts add up to the snapshots. The scored
/// come latest first.
pub fn match_results(
    snapshots: Vec<Snapshot>,
    games: &HashMap<GameKey, Option<Outcome>>,
) -> (Vec<Scored>, usize, usize) {
    let (mut pending, mut unmatched) = (0, 0);
    let mut scored: Vec<Scored> = Vec::new();
    for s in snapshots {
        let key = (s.game_date, s.home_code.clone(), s.away_code.clone());
        match games.get(&key) {
            // A tied score, which the sport does not have, is a data error: unmatched.
            Some(Some(outcome)) => match tracking::score(s, *outcome) {
                Some(x) => scored.push(x),
                None => unmatched += 1,
            },
            Some(None) => pending += 1,
            None => unmatched += 1,
        }
    }
    scored.sort_by(|a, b| {
        b.snapshot
            .game_date
            .cmp(&a.snapshot.game_date)
            .then_with(|| a.snapshot.home_code.cmp(&b.snapshot.home_code))
    });
    (scored, pending, unmatched)
}

/// The record of `season`: each stored snapshot matched to its game by date and team
/// codes and scored once the game is played.
pub async fn record(app: &App, season: SeasonYear) -> Result<TrackRecord, SeasonLoadError> {
    let snapshots = app.store.snapshots(season).await?;
    let current_model = tracking::tonight_model_version();
    if snapshots.is_empty() {
        return Ok(TrackRecord {
            season,
            snapshots: 0,
            scored: 0,
            pending: 0,
            unmatched: 0,
            overall: Summary::default(),
            by_model: vec![],
            by_month: vec![],
            games: vec![],
            current_model,
            note: Some(
                "No snapshot is stored for this season: they are captured before each game's start while the application runs with the ESPN feed on."
                    .to_string(),
            ),
        });
    }
    let data = app.season_data(season).await?;
    let codes: HashMap<TeamId, String> = app
        .store
        .team_code_map()
        .await?
        .into_iter()
        .map(|(code, team)| (team.id, code))
        .collect();
    let games: HashMap<GameKey, Option<Outcome>> = data
        .games
        .iter()
        .filter_map(|g| {
            let home = codes.get(&g.home_team_id)?.clone();
            let away = codes.get(&g.away_team_id)?.clone();
            let outcome = g.home_score.zip(g.away_score).map(|(h, a)| Outcome {
                home_score: h,
                away_score: a,
            });
            Some(((g.game_date, home, away), outcome))
        })
        .collect();
    let total = snapshots.len();
    let (mut scored, pending, unmatched) = match_results(snapshots, &games);
    let overall = tracking::summarize(&scored);
    let by_model = tracking::group_by(&scored, |g| g.snapshot.model_version.clone());
    let by_month = tracking::group_by(&scored, |g| month_of(g.snapshot.game_date));
    let n = scored.len();
    scored.truncate(MAX_GAMES_LISTED);
    Ok(TrackRecord {
        season,
        snapshots: total,
        scored: n,
        pending,
        unmatched,
        overall,
        by_model,
        by_month,
        games: scored,
        current_model,
        note: None,
    })
}

fn month_of(d: NaiveDate) -> String {
    format!("{}-{:02}", d.year(), d.month())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::market::GameMarket;
    use crate::application::worth::{GameTonight, TeamTonight};
    use crate::domain::worth::NightGame;
    use chrono::{DateTime, Utc};

    fn at(s: i64) -> DateTime<Utc> {
        DateTime::<Utc>::from_timestamp(s, 0).unwrap()
    }

    fn night() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 11, 1).unwrap()
    }

    fn game(id: i64, home: i32, away: i32) -> GameTonight {
        GameTonight {
            game: NightGame {
                game_id: id,
                home_team_id: TeamId::new(home),
                away_team_id: TeamId::new(away),
                neutral: false,
                home_rated: true,
                away_rated: true,
                home_rating: 1497.0,
                away_rating: 1497.0,
                home_penalty: 58.0,
                away_penalty: 17.0,
                home_tonight: 1439.0,
                away_tonight: 1480.0,
                p_home_full: 0.56,
                p_home_tonight: 0.5,
                home_margin_full: 1.5,
                home_margin_tonight: 0.0,
                home_moneyline_tonight: -101.0,
                away_moneyline_tonight: 101.0,
            },
            home_code: format!("H{home}"),
            away_code: format!("A{away}"),
        }
    }

    fn team(id: i32, stored: f64) -> TeamTonight {
        TeamTonight {
            team_id: TeamId::new(id),
            code: format!("T{id}"),
            name: String::new(),
            rating: 1497.0,
            stored_rating: stored,
            games_short: 0,
            penalty: 0.0,
            rating_tonight: 1497.0,
            out: vec![],
            opponent: None,
            home: None,
        }
    }

    fn tonight(games: Vec<GameTonight>, teams: Vec<TeamTonight>) -> Tonight {
        Tonight {
            season: SeasonYear::of(2027),
            as_of: night(),
            night: Some(night()),
            games_played: 0,
            report_date: Some(night()),
            games_played_short: 0,
            rating_model: "test",
            home_advantage: 41.0,
            home_advantage_fitted: 41.0,
            games,
            teams,
            note: None,
            absences: vec![],
        }
    }

    fn line(id: i64, start: Option<DateTime<Utc>>, started: bool) -> GameMarket {
        GameMarket {
            game_id: id,
            provider: "DraftKings".into(),
            home_spread: Some(-2.5),
            home_spread_odds: Some(-105.0),
            away_spread_odds: Some(-115.0),
            home_moneyline: Some(-135.0),
            away_moneyline: Some(114.0),
            total: Some(219.5),
            p_home: Some(0.55),
            p_away: Some(0.45),
            book_margin: Some(0.046),
            start_time: start,
            started,
        }
    }

    #[test]
    fn a_snapshot_is_taken_only_of_a_priced_game_read_before_its_start() {
        let read = at(1_000);
        let t = tonight(
            vec![
                game(1, 1, 2),
                game(2, 3, 4),
                game(3, 5, 6),
                game(4, 7, 8),
                game(5, 9, 10),
            ],
            vec![team(1, 1494.0)],
        );
        let m = Market {
            season: SeasonYear::of(2027),
            night: Some(night()),
            read_at: Some(read),
            games: vec![
                // Game 1 starts later: kept. Game 2 started per the source; game 3's start
                // is past at the read; game 4 has no start time and is not started: kept.
                line(1, Some(at(5_000)), false),
                line(2, Some(at(5_000)), true),
                line(3, Some(at(900)), false),
                line(4, None, false),
            ],
            note: None,
        };
        let s = build_snapshots(SeasonYear::of(2027), &t, &m, "espn", "m1");
        let ids: Vec<&str> = s.iter().map(|x| x.home_code.as_str()).collect();
        assert_eq!(ids, ["H1", "H7"]);
        let first = &s[0];
        assert_eq!((first.game_date, first.captured_at), (night(), read));
        // The stored rating from the team table; a team missing from it falls back to the
        // rating of the game.
        assert_eq!((first.home_stored, first.away_stored), (1494.0, 1497.0));
        assert_eq!(
            (first.p_home_model, first.p_home_market, first.home_spread),
            (0.5, Some(0.55), Some(-2.5))
        );
        assert_eq!(
            (first.source.as_str(), first.model_version.as_str()),
            ("espn", "m1")
        );
        // Without a read time or a night there is nothing to stamp.
        let unread = Market {
            read_at: None,
            ..m.clone()
        };
        assert!(build_snapshots(SeasonYear::of(2027), &t, &unread, "espn", "m1").is_empty());
    }

    #[test]
    fn snapshots_are_scored_pending_or_unmatched_and_the_scored_come_latest_first() {
        let t = tonight(vec![game(1, 1, 2)], vec![]);
        let m = Market {
            season: SeasonYear::of(2027),
            night: Some(night()),
            read_at: Some(at(1)),
            games: vec![line(1, None, false)],
            note: None,
        };
        let base = build_snapshots(SeasonYear::of(2027), &t, &m, "espn", "m1").remove(0);
        let on = |day: u32, home: &str| Snapshot {
            game_date: NaiveDate::from_ymd_opt(2026, 11, day).unwrap(),
            home_code: home.into(),
            ..base.clone()
        };
        let key = |day: u32, home: &str| {
            (
                NaiveDate::from_ymd_opt(2026, 11, day).unwrap(),
                home.to_string(),
                "A2".to_string(),
            )
        };
        let played = Some(Outcome {
            home_score: 100,
            away_score: 104,
        });
        let games: HashMap<GameKey, Option<Outcome>> = [
            (key(1, "H1"), played),
            (key(3, "H1"), played),
            (key(4, "H1"), None),
            (
                key(5, "H1"),
                Some(Outcome {
                    home_score: 99,
                    away_score: 99,
                }),
            ),
        ]
        .into();
        let (scored, pending, unmatched) = match_results(
            vec![
                on(1, "H1"),
                on(3, "H1"),
                on(4, "H1"),
                on(2, "H9"),
                on(5, "H1"),
            ],
            &games,
        );
        // The tie counts as unmatched, so the three counts add up to the five.
        assert_eq!((scored.len(), pending, unmatched), (2, 1, 2));
        let dates: Vec<u32> = scored.iter().map(|s| s.snapshot.game_date.day()).collect();
        assert_eq!(dates, [3, 1]);
        assert_eq!(month_of(night()), "2026-11");
    }
}
