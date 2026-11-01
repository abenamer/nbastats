//! Tonight beside the market: the night's sportsbook lines, read from the market source
//! (ESPN's scoreboard, which carries one sportsbook's lines for games not yet played), set
//! against the Tonight panel's games. Each game gets the sportsbook's spread, moneylines
//! and total, and the chances its moneylines imply with the bookmaker's margin taken out
//! (`domain::market`). Nothing is stored: the lines are read when asked for and kept
//! `LINES_TTL` (a failure `FAILURE_TTL`), one read at a time, so reloads and concurrent
//! requests ask the source at most once per date in that time.

use std::collections::HashMap;
use std::sync::{Arc, PoisonError};
use std::time::{Duration, Instant};

use chrono::{DateTime, NaiveDate, Utc};
use serde::Serialize;
use tracing::warn;

use crate::domain::ids::SeasonYear;
use crate::domain::market;

use super::ports::{GameLog, MarketLine, MarketSource};
use super::reports::codes_of;
use super::state::{App, SeasonLoadError};
use super::worth::{self, GameTonight};

/// How long the lines read for a date are reused.
pub const LINES_TTL: Duration = Duration::from_secs(5 * 60);
/// How long a failed read is remembered before the source is asked again.
pub const FAILURE_TTL: Duration = Duration::from_secs(60);

/// A read of the lines failed; the detail is in the log, and the failure is remembered
/// `FAILURE_TTL`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("the market lines could not be read")]
pub struct LinesUnavailable;

/// What a read of a date gave: the lines with when they were read, or that it failed.
type Read = Result<(DateTime<Utc>, Arc<Vec<MarketLine>>), LinesUnavailable>;

/// The lines read per date, kept for a while, with one read from the source at a time: a
/// request that finds no fresh entry waits its turn, then looks again before asking. The
/// one slot serves every date; the panel reads one night, so a per-date slot would buy
/// nothing but code.
#[derive(Debug)]
pub struct LinesCache {
    entries: std::sync::Mutex<HashMap<NaiveDate, (Instant, Read)>>,
    slot: tokio::sync::Mutex<()>,
    ttl: Duration,
    failure_ttl: Duration,
}

impl Default for LinesCache {
    fn default() -> Self {
        Self::new(LINES_TTL, FAILURE_TTL)
    }
}

impl LinesCache {
    pub fn new(ttl: Duration, failure_ttl: Duration) -> Self {
        Self {
            entries: std::sync::Mutex::new(HashMap::new()),
            slot: tokio::sync::Mutex::new(()),
            ttl,
            failure_ttl,
        }
    }

    fn fresh(&self, date: NaiveDate) -> Option<Read> {
        let entries = self.entries.lock().unwrap_or_else(PoisonError::into_inner);
        let (at, read) = entries.get(&date)?;
        let ttl = if read.is_ok() {
            self.ttl
        } else {
            self.failure_ttl
        };
        (at.elapsed() < ttl).then(|| read.clone())
    }

    /// The lines for `date`: kept ones while fresh, else one read from `source`.
    pub async fn lines<M: MarketSource>(&self, source: &M, date: NaiveDate) -> Read {
        if let Some(read) = self.fresh(date) {
            return read;
        }
        let _turn = self.slot.lock().await;
        // Another request may have read the date while this one waited.
        if let Some(read) = self.fresh(date) {
            return read;
        }
        let read: Read = match source.lines_on(date).await {
            Ok(lines) => Ok((Utc::now(), Arc::new(lines))),
            Err(e) => {
                warn!(%date, error = %e, "market lines could not be read");
                Err(LinesUnavailable)
            }
        };
        let mut entries = self.entries.lock().unwrap_or_else(PoisonError::into_inner);
        // Only recent dates are kept: the night changes once a day.
        let keep = self.ttl.max(self.failure_ttl);
        entries.retain(|_, (at, _)| at.elapsed() < keep);
        entries.insert(date, (Instant::now(), read.clone()));
        read
    }
}

/// One game of the night as the sportsbook prices it, the home side being the panel's.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GameMarket {
    pub game_id: i64,
    pub provider: String,
    /// The home team's spread, negative when it gives points.
    pub home_spread: Option<f64>,
    pub home_spread_odds: Option<f64>,
    pub away_spread_odds: Option<f64>,
    pub home_moneyline: Option<f64>,
    pub away_moneyline: Option<f64>,
    pub total: Option<f64>,
    /// The chances the moneylines imply with the bookmaker's margin taken out, and that
    /// margin (named so as not to read as a margin of points).
    pub p_home: Option<f64>,
    pub p_away: Option<f64>,
    pub book_margin: Option<f64>,
}

/// The night's market: the games the source prices, matched to the panel's games.
#[derive(Debug, Clone, Serialize)]
pub struct Market {
    pub season: SeasonYear,
    pub night: Option<NaiveDate>,
    /// When the lines were read from the source.
    pub read_at: Option<DateTime<Utc>>,
    pub games: Vec<GameMarket>,
    /// Why there are no lines, when there are none.
    pub note: Option<String>,
}

/// Match the source's lines to the night's games by team: a line refers to a game when
/// its sides can name the game's teams, in either order. A line listed the other way
/// round (sources disagree on the home team at a neutral site) is turned to the panel's
/// home side.
pub fn match_lines(
    lines: &[MarketLine],
    games: &[GameTonight],
    by_name: &HashMap<String, String>,
) -> Vec<GameMarket> {
    games
        .iter()
        .filter_map(|g| {
            lines.iter().find_map(|l| {
                let h = codes_of(&l.home_abbr, &l.home_name, by_name);
                let a = codes_of(&l.away_abbr, &l.away_name, by_name);
                let same = h.contains(&g.home_code) && a.contains(&g.away_code);
                let turned = h.contains(&g.away_code) && a.contains(&g.home_code);
                (same || turned).then(|| priced(g.game.game_id, l, turned))
            })
        })
        .collect()
}

fn priced(game_id: i64, l: &MarketLine, turned: bool) -> GameMarket {
    let (home_spread, home_spread_odds, away_spread_odds, home_ml, away_ml) = if turned {
        (
            l.home_spread.map(|s| -s),
            l.away_spread_odds,
            l.home_spread_odds,
            l.away_moneyline,
            l.home_moneyline,
        )
    } else {
        (
            l.home_spread,
            l.home_spread_odds,
            l.away_spread_odds,
            l.home_moneyline,
            l.away_moneyline,
        )
    };
    let fair = home_ml
        .zip(away_ml)
        .and_then(|(h, a)| market::without_margin(h, a));
    GameMarket {
        game_id,
        provider: l.provider.clone(),
        home_spread,
        home_spread_odds,
        away_spread_odds,
        home_moneyline: home_ml,
        away_moneyline: away_ml,
        total: l.total,
        p_home: fair.map(|f| f.home),
        p_away: fair.map(|f| f.away),
        book_margin: fair.map(|f| f.margin),
    }
}

/// Why a night read from the source has no line for any of its games.
fn unmatched_note(night: NaiveDate, lines_listed: usize) -> String {
    if lines_listed == 0 {
        format!("the source lists no lines for {night}: they appear before a game and go once it starts")
    } else {
        format!("none of the source's {lines_listed} lines for {night} matched the night's games")
    }
}

/// The market for the next night with games of `season`, beside the panel's games.
pub async fn tonight_market(app: &App, season: SeasonYear) -> Result<Market, SeasonLoadError> {
    let t = worth::tonight(app, season, None).await?;
    let empty = |note: String| Market {
        season,
        night: t.night,
        read_at: None,
        games: Vec::new(),
        note: Some(note),
    };
    let Some(night) = t.night else {
        return Ok(empty(
            "no regular-season game remains, so there are no lines to compare".to_string(),
        ));
    };
    let Some(source) = app.reports.as_ref() else {
        return Ok(empty(
            "the market lines come from ESPN's scoreboard, which is off (GAME_REPORTS_ENABLED=false)"
                .to_string(),
        ));
    };
    let Ok((read_at, lines)) = app.market_lines.lines(source, night).await else {
        return Ok(empty(format!(
            "the lines could not be read from ESPN's scoreboard; the next try is within {} seconds",
            FAILURE_TTL.as_secs()
        )));
    };
    let by_name: HashMap<String, String> = app
        .store
        .team_code_map()
        .await?
        .into_iter()
        .map(|(code, team)| (team.name.trim().to_ascii_lowercase(), code))
        .collect();
    let games = match_lines(&lines, &t.games, &by_name);
    let note = games.is_empty().then(|| unmatched_note(night, lines.len()));
    Ok(Market {
        season,
        night: Some(night),
        read_at: Some(read_at),
        games,
        note,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ids::TeamId;
    use crate::domain::worth::NightGame;

    fn game(id: i64, home: &str, away: &str) -> GameTonight {
        GameTonight {
            game: NightGame {
                game_id: id,
                home_team_id: TeamId::new(1),
                away_team_id: TeamId::new(2),
                neutral: false,
                home_rated: true,
                away_rated: true,
                home_rating: 1500.0,
                away_rating: 1500.0,
                home_penalty: 0.0,
                away_penalty: 0.0,
                home_tonight: 1500.0,
                away_tonight: 1500.0,
                p_home_full: 0.5,
                p_home_tonight: 0.5,
                home_margin_full: 0.0,
                home_margin_tonight: 0.0,
                home_moneyline_tonight: 100.0,
                away_moneyline_tonight: 100.0,
            },
            home_code: home.to_string(),
            away_code: away.to_string(),
        }
    }

    fn line(home: (&str, &str), away: (&str, &str)) -> MarketLine {
        MarketLine {
            home_abbr: home.0.into(),
            away_abbr: away.0.into(),
            home_name: home.1.into(),
            away_name: away.1.into(),
            provider: "DraftKings".into(),
            home_spread: Some(-2.5),
            home_spread_odds: Some(-105.0),
            away_spread_odds: Some(-115.0),
            home_moneyline: Some(-135.0),
            away_moneyline: Some(114.0),
            total: Some(219.5),
        }
    }

    /// A source that counts its reads and fails when told to.
    struct Counting {
        reads: std::sync::atomic::AtomicUsize,
        fail: bool,
    }

    impl MarketSource for Counting {
        async fn lines_on(
            &self,
            _date: NaiveDate,
        ) -> Result<Vec<MarketLine>, super::super::ports::SourceError> {
            self.reads.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            // A real read waits on the network: yield, so a concurrent request reaches
            // the slot while this one holds it.
            tokio::task::yield_now().await;
            if self.fail {
                Err(super::super::ports::SourceError::Transport("down".into()))
            } else {
                Ok(vec![line(
                    ("GS", "Golden State Warriors"),
                    ("LAC", "LA Clippers"),
                )])
            }
        }
    }

    fn counting(fail: bool) -> Counting {
        Counting {
            reads: std::sync::atomic::AtomicUsize::new(0),
            fail,
        }
    }

    #[tokio::test]
    async fn the_cache_reads_a_date_once_while_fresh_and_remembers_a_failure() {
        let reads = |s: &Counting| s.reads.load(std::sync::atomic::Ordering::SeqCst);
        let d = NaiveDate::from_ymd_opt(2026, 11, 1).unwrap();
        let cache = LinesCache::default();
        let ok = counting(false);
        // Concurrent requests for a cold date make one read; a later one uses it.
        let (a, b) = tokio::join!(cache.lines(&ok, d), cache.lines(&ok, d));
        assert!(a.is_ok() && b.is_ok());
        assert_eq!(cache.lines(&ok, d).await.unwrap().1.len(), 1);
        assert_eq!(reads(&ok), 1);
        // A failure is remembered too, so a failing source is not asked on every load.
        let down = counting(true);
        let other = d.succ_opt().unwrap();
        assert!(cache.lines(&down, other).await.is_err());
        assert!(cache.lines(&down, other).await.is_err());
        assert_eq!(reads(&down), 1);
        // With nothing kept, every request reads.
        let fresh = LinesCache::new(Duration::ZERO, Duration::ZERO);
        let ok = counting(false);
        fresh.lines(&ok, d).await.unwrap();
        fresh.lines(&ok, d).await.unwrap();
        assert_eq!(reads(&ok), 2);
    }

    #[test]
    fn a_night_without_matched_lines_says_why() {
        let d = NaiveDate::from_ymd_opt(2026, 11, 1).unwrap();
        assert!(unmatched_note(d, 0).contains("no lines for 2026-11-01"));
        assert!(unmatched_note(d, 3).contains("none of the source's 3 lines"));
    }

    #[test]
    fn the_lines_find_their_games_by_team_and_turn_when_listed_the_other_way() {
        let by_name: HashMap<String, String> = [
            ("golden state warriors", "GSW"),
            ("los angeles clippers", "LAC"),
        ]
        .into_iter()
        .map(|(n, c)| (n.to_string(), c.to_string()))
        .collect();
        // ESPN's GS maps to GSW, and LA Clippers by its abbreviation.
        let lines = vec![line(
            ("GS", "Golden State Warriors"),
            ("LAC", "LA Clippers"),
        )];
        let m = match_lines(
            &lines,
            &[game(7, "GSW", "LAC"), game(8, "BOS", "ORL")],
            &by_name,
        );
        assert_eq!(m.len(), 1);
        let g = &m[0];
        assert_eq!(
            (g.game_id, g.home_spread, g.home_moneyline),
            (7, Some(-2.5), Some(-135.0))
        );
        let p = g.p_home.unwrap();
        assert!(
            (p + g.p_away.unwrap() - 1.0).abs() < 1e-12 && p > 0.55 && p < 0.56,
            "{p}"
        );
        assert!(g.book_margin.unwrap() > 0.0);
        // The same line against a game with the sides the other way round turns to it.
        let m = match_lines(&lines, &[game(9, "LAC", "GSW")], &by_name);
        let g = &m[0];
        assert_eq!(
            (g.home_spread, g.home_spread_odds, g.away_spread_odds),
            (Some(2.5), Some(-115.0), Some(-105.0))
        );
        assert_eq!(
            (g.home_moneyline, g.away_moneyline),
            (Some(114.0), Some(-135.0))
        );
        assert!(g.p_home.unwrap() < 0.45);
    }
}
