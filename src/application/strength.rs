//! The backward view of the Alpha Simulator: a title record read as an alpha and as a
//! strength against the actual playoff fields of a window of seasons.
//!
//! The use case builds each season's field from the stored team-seasons (every NBA team's
//! pre-playoff rating, the franchise's own left out), hands them to the pure readings in
//! `domain::simulation::exact` and `domain::strength`, and runs the exact brackets on a
//! blocking thread under one slot, because a long window costs seconds of CPU.

use serde::Serialize;

use crate::domain::elo;
use crate::domain::ids::SeasonYear;
use crate::domain::records::{League, TeamSeasonStat};
use crate::domain::simulation::exact::{self, Strength};
use crate::domain::strength::{elo_strength, EloStrength, Field};

use super::ports::{AnalysisStore, StoreError};

/// The longest window, in seasons: each season costs a few dozen exact brackets per gap
/// searched, so forty seasons answer in a few seconds.
pub const MAX_WINDOW: i32 = 40;
/// The fewest rated teams for a season to count as a field: a 16-team bracket.
const MIN_FIELD: usize = 16;
/// The franchise count the alpha reading falls back to when no season has a field.
const DEFAULT_FRANCHISES: usize = 30;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Record {
    /// The franchise asked about, by key; `None` for a franchise from outside the league.
    pub franchise: Option<String>,
    pub from: SeasonYear,
    pub to: SeasonYear,
    pub titles: usize,
}

#[derive(Debug, thiserror::Error)]
pub enum StrengthError {
    #[error("{0}")]
    Invalid(String),
    #[error("another record is being read; try again in a moment")]
    Busy,
    #[error(transparent)]
    Store(#[from] StoreError),
}

/// What the franchise actually did over the window, for the comparison on the page.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Actual {
    pub franchise: Option<String>,
    /// Seasons of the window with a field.
    pub seasons_with_field: usize,
    /// Seasons in which the franchise had a pre-playoff rating.
    pub seasons_with_rating: usize,
    /// The franchise's mean gap above the league mean over those seasons.
    pub mean_gap: Option<f64>,
    /// The best team's mean gap above the league mean over the seasons with a field.
    pub best_mean_gap: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Answer {
    pub alpha: Strength,
    /// `None` when no season of the window has a field.
    pub elo: Option<EloStrength>,
    pub actual: Actual,
}

/// The fields and the context the window gives, from the team-seasons.
struct Window {
    fields: Vec<Field>,
    own_gaps: Vec<f64>,
    best_gaps: Vec<f64>,
    /// Teams in the last season with a field: the franchise count of the alpha reading.
    franchises: usize,
}

fn window(stats: &[TeamSeasonStat], record: &Record) -> Window {
    let key = record.franchise.as_deref();
    let mut w = Window {
        fields: Vec::new(),
        own_gaps: Vec::new(),
        best_gaps: Vec::new(),
        franchises: 0,
    };
    for season in SeasonYear::range(record.from, record.to) {
        let rows: Vec<(&str, f64)> = stats
            .iter()
            .filter(|s| s.season == season && League::of_code(&s.code) == League::Nba)
            .filter_map(|s| s.elo_pre_playoffs.map(|e| (s.franchise_key.as_str(), e)))
            .collect();
        if rows.len() < MIN_FIELD {
            continue;
        }
        let mean = rows.iter().map(|(_, e)| e).sum::<f64>() / rows.len() as f64;
        let best = rows.iter().map(|(_, e)| *e).fold(f64::MIN, f64::max);
        w.best_gaps.push(best - mean);
        if let Some((_, own)) = rows.iter().find(|(k, _)| Some(*k) == key) {
            w.own_gaps.push(own - mean);
        }
        w.franchises = rows.len();
        w.fields.push(Field {
            season,
            mean,
            others: rows
                .iter()
                .filter(|(k, _)| Some(*k) != key)
                .map(|(_, e)| *e)
                .collect(),
        });
    }
    if w.franchises == 0 {
        w.franchises = DEFAULT_FRANCHISES;
    }
    w
}

fn mean_of(values: &[f64]) -> Option<f64> {
    (!values.is_empty()).then(|| values.iter().sum::<f64>() / values.len() as f64)
}

/// Validate the record and read it. A franchise key must name a franchise in the store.
pub async fn read_record<S: AnalysisStore>(
    store: &S,
    slot: &tokio::sync::Semaphore,
    record: Record,
) -> Result<Answer, StrengthError> {
    if record.from > record.to {
        return Err(StrengthError::Invalid("from must not be after to".into()));
    }
    let seasons = record.to.end_year() - record.from.end_year() + 1;
    if seasons > MAX_WINDOW {
        return Err(StrengthError::Invalid(format!(
            "the window may span at most {MAX_WINDOW} seasons"
        )));
    }
    let seasons = seasons as usize;
    if record.titles == 0 || record.titles > seasons {
        return Err(StrengthError::Invalid(
            "titles must be between 1 and the number of seasons in the window".into(),
        ));
    }
    let _slot = slot.try_acquire().map_err(|_| StrengthError::Busy)?;
    let stats = store.team_season_stats().await?;
    if let Some(key) = record.franchise.as_deref() {
        if !stats.iter().any(|s| s.franchise_key == key) {
            return Err(StrengthError::Invalid(format!("unknown franchise {key:?}")));
        }
    }
    let titles = record.titles;
    tokio::task::spawn_blocking(move || {
        let w = window(&stats, &record);
        let alpha = exact::strength_for_titles(w.franchises.max(2), seasons, titles)
            .ok_or_else(|| StrengthError::Invalid("no strength for these inputs".into()))?;
        let elo = elo_strength(&alpha, &w.fields, elo::DEFAULT_HOME_ADVANTAGE);
        Ok(Answer {
            alpha,
            elo,
            actual: Actual {
                franchise: record.franchise,
                seasons_with_field: w.fields.len(),
                seasons_with_rating: w.own_gaps.len(),
                mean_gap: mean_of(&w.own_gaps),
                best_mean_gap: mean_of(&w.best_gaps),
            },
        })
    })
    .await
    .map_err(|e| StrengthError::Invalid(format!("the reading did not finish: {e}")))?
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::models::{GameType, PlayoffResult};
    use chrono::NaiveDate;

    fn stat(season: i32, code: &str, pre: Option<f64>) -> TeamSeasonStat {
        TeamSeasonStat {
            season: SeasonYear::of(season),
            code: code.to_string(),
            name: code.to_string(),
            franchise_key: code.to_string(),
            games: 82,
            peak: 1600.0,
            peak_date: NaiveDate::from_ymd_opt(season, 5, 1).unwrap(),
            peak_game_type: GameType::Regular,
            low: 1400.0,
            low_date: NaiveDate::from_ymd_opt(season - 1, 11, 1).unwrap(),
            low_game_type: GameType::Regular,
            wins: 41,
            losses: 41,
            playoff_wins: 0,
            playoff_losses: 0,
            elo_start: Some(1500.0),
            elo_pre_playoffs: pre,
            elo_end: pre,
            playoff_result: PlayoffResult::Missed,
            conference: None,
            division: None,
        }
    }

    #[test]
    fn a_window_takes_the_rated_seasons_and_leaves_the_franchise_out() {
        let mut stats = Vec::new();
        for season in [2015, 2016] {
            for i in 0..20 {
                let pre = if i == 0 { 1800.0 } else { 1500.0 + i as f64 };
                stats.push(stat(season, &format!("T{i:02}"), Some(pre)));
            }
        }
        // 2017 has too few rated teams to be a field; an ABA team never counts.
        for i in 0..10 {
            stats.push(stat(2017, &format!("T{i:02}"), Some(1500.0)));
        }
        stats.push(stat(2016, "INA", Some(1900.0)));
        let record = Record {
            franchise: Some("T00".into()),
            from: SeasonYear::of(2015),
            to: SeasonYear::of(2017),
            titles: 2,
        };
        let w = window(&stats, &record);
        assert_eq!(w.fields.len(), 2);
        assert_eq!(w.franchises, 20);
        assert_eq!(w.own_gaps.len(), 2);
        assert!(w.fields.iter().all(|f| f.others.len() == 19));
        assert!(w.fields.iter().all(|f| !f.others.contains(&1800.0)));
        // The field's mean includes the 1800 team: (1800 + 19 teams near 1510) / 20 = 1524.5.
        assert!(w.own_gaps.iter().all(|g| (*g - 275.5).abs() < 1e-9));
        assert_eq!(w.best_gaps, w.own_gaps);
        // No rated season at all: no field, the default franchise count.
        let none = window(
            &stats,
            &Record {
                from: SeasonYear::of(2017),
                to: SeasonYear::of(2017),
                ..record
            },
        );
        assert!(none.fields.is_empty());
        assert_eq!(none.franchises, DEFAULT_FRANCHISES);
    }
}
