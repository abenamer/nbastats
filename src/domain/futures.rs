//! Title futures: the sportsbook's chance that each team wins the title, read from its
//! outright prices, beside the model's chance from a forecast of the same day, saved once a
//! day and scored against the champion once the season is decided. The capture and the
//! store are the application's; this module holds the records and the scoring.

use std::collections::BTreeMap;

use chrono::{DateTime, NaiveDate, Utc};
use serde::Serialize;

use crate::domain::ids::SeasonYear;

/// One team's title chances on one day: the sportsbook's price and the chance it implies
/// with the margin of the whole field taken out, and the model's chance with its error.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FuturesSnapshot {
    pub season: SeasonYear,
    pub snapshot_date: NaiveDate,
    pub team_code: String,
    pub captured_at: DateTime<Utc>,
    pub provider: String,
    pub source: String,
    /// The sportsbook's American price on the team to win the title.
    pub price: f64,
    /// The chance the price implies once the field's margin is taken out.
    pub p_market: f64,
    /// The field's margin: the implied chances' excess over 100%.
    pub book_margin: f64,
    /// The model's chance from the day's forecast, and its simulation error.
    pub p_model: f64,
    pub p_model_se: f64,
    pub model_version: String,
    pub app_version: String,
}

/// One day's field scored against the champion: the squared error summed over the teams
/// (the multi-outcome Brier score) and minus the log of the chance given to the champion,
/// for the model and for the sportsbook. Lower is better.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DayScore {
    pub date: NaiveDate,
    pub teams: usize,
    pub brier_model: f64,
    pub brier_market: f64,
    pub log_loss_model: f64,
    pub log_loss_market: f64,
    /// The chance each gave the champion that day.
    pub champion_p_model: f64,
    pub champion_p_market: f64,
}

/// The chance floor the log loss is taken at, so a team given nothing scores finite.
const LOG_FLOOR: f64 = 1e-6;

/// Score each day's field against `champion`, the code of the team that won the title.
/// A day whose field lacks the champion is skipped: a field without the champion was not
/// a field.
pub fn score_days(snapshots: &[FuturesSnapshot], champion: &str) -> Vec<DayScore> {
    let mut by_day: BTreeMap<NaiveDate, Vec<&FuturesSnapshot>> = BTreeMap::new();
    for s in snapshots {
        by_day.entry(s.snapshot_date).or_default().push(s);
    }
    by_day
        .into_iter()
        .filter_map(|(date, rows)| {
            let winner = rows.iter().find(|r| r.team_code == champion)?;
            let (mut bm, mut bk) = (0.0, 0.0);
            for r in &rows {
                let y = if r.team_code == champion { 1.0 } else { 0.0 };
                bm += (r.p_model - y).powi(2);
                bk += (r.p_market - y).powi(2);
            }
            Some(DayScore {
                date,
                teams: rows.len(),
                brier_model: bm,
                brier_market: bk,
                log_loss_model: -winner.p_model.max(LOG_FLOOR).ln(),
                log_loss_market: -winner.p_market.max(LOG_FLOOR).ln(),
                champion_p_model: winner.p_model,
                champion_p_market: winner.p_market,
            })
        })
        .collect()
}

/// A season's record of the model against the sportsbook on the title, over the days
/// scored.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Record {
    pub days: usize,
    /// Days the model's Brier score was the lower, the sportsbook's, and ties.
    pub model_closer: usize,
    pub market_closer: usize,
    pub ties: usize,
    pub mean_brier_model: f64,
    pub mean_brier_market: f64,
    pub mean_log_loss_model: f64,
    pub mean_log_loss_market: f64,
    /// The first and the last day scored.
    pub first: Option<DayScore>,
    pub last: Option<DayScore>,
}

/// Two scores within this of each other are a tie.
const LEVEL_GAP: f64 = 1e-12;

pub fn summarize(days: &[DayScore]) -> Record {
    let n = days.len();
    let mean = |f: fn(&DayScore) -> f64| {
        if n == 0 {
            0.0
        } else {
            days.iter().map(f).sum::<f64>() / n as f64
        }
    };
    let (mut model, mut market, mut ties) = (0, 0, 0);
    for d in days {
        if (d.brier_model - d.brier_market).abs() <= LEVEL_GAP {
            ties += 1;
        } else if d.brier_model < d.brier_market {
            model += 1;
        } else {
            market += 1;
        }
    }
    Record {
        days: n,
        model_closer: model,
        market_closer: market,
        ties,
        mean_brier_model: mean(|d| d.brier_model),
        mean_brier_market: mean(|d| d.brier_market),
        mean_log_loss_model: mean(|d| d.log_loss_model),
        mean_log_loss_market: mean(|d| d.log_loss_market),
        first: days.first().cloned(),
        last: days.last().cloned(),
    }
}

/// The latest day's field: its date and its rows, the model's chance first.
pub fn latest_day(snapshots: &[FuturesSnapshot]) -> Option<(NaiveDate, Vec<&FuturesSnapshot>)> {
    let date = snapshots.iter().map(|s| s.snapshot_date).max()?;
    let mut rows: Vec<&FuturesSnapshot> = snapshots
        .iter()
        .filter(|s| s.snapshot_date == date)
        .collect();
    rows.sort_by(|a, b| {
        b.p_model
            .total_cmp(&a.p_model)
            .then_with(|| a.team_code.cmp(&b.team_code))
    });
    Some((date, rows))
}

/// The distinct days with a field, earliest first.
pub fn days_of(snapshots: &[FuturesSnapshot]) -> Vec<NaiveDate> {
    let mut days: Vec<NaiveDate> = snapshots.iter().map(|s| s.snapshot_date).collect();
    days.sort();
    days.dedup();
    days
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snap(date: (i32, u32, u32), code: &str, p_model: f64, p_market: f64) -> FuturesSnapshot {
        FuturesSnapshot {
            season: SeasonYear::of(2027),
            snapshot_date: NaiveDate::from_ymd_opt(date.0, date.1, date.2).unwrap(),
            team_code: code.into(),
            captured_at: DateTime::<Utc>::from_timestamp(1_800_000_000, 0).unwrap(),
            provider: "FanDuel".into(),
            source: "the-odds-api".into(),
            price: 400.0,
            p_market,
            book_margin: 0.3,
            p_model,
            p_model_se: 0.01,
            model_version: "m".into(),
            app_version: "0.1.0".into(),
        }
    }

    #[test]
    fn each_day_is_scored_against_the_champion_and_a_field_without_him_is_skipped() {
        let s = vec![
            snap((2026, 11, 1), "OKC", 0.30, 0.20),
            snap((2026, 11, 1), "BOS", 0.20, 0.30),
            snap((2026, 11, 1), "DEN", 0.50, 0.50),
            snap((2026, 11, 2), "OKC", 0.40, 0.25),
            snap((2026, 11, 2), "BOS", 0.10, 0.25),
            snap((2026, 11, 2), "DEN", 0.50, 0.50),
            snap((2026, 11, 3), "BOS", 0.5, 0.5),
            snap((2026, 11, 3), "DEN", 0.5, 0.5),
        ];
        let days = score_days(&s, "OKC");
        assert_eq!(days.len(), 2, "the third day has no OKC row");
        let d1 = &days[0];
        assert_eq!(d1.teams, 3);
        // Model: (0.3-1)^2 + 0.2^2 + 0.5^2 = 0.49 + 0.04 + 0.25 = 0.78.
        assert!((d1.brier_model - 0.78).abs() < 1e-12);
        // Market: (0.2-1)^2 + 0.3^2 + 0.5^2 = 0.64 + 0.09 + 0.25 = 0.98.
        assert!((d1.brier_market - 0.98).abs() < 1e-12);
        assert!((d1.log_loss_model - (-(0.3f64).ln())).abs() < 1e-12);
        assert_eq!(d1.champion_p_market, 0.20);
        let r = summarize(&days);
        assert_eq!(
            (r.days, r.model_closer, r.market_closer, r.ties),
            (2, 2, 0, 0)
        );
        assert!((r.mean_brier_model - (0.78 + (0.36 + 0.01 + 0.25)) / 2.0).abs() < 1e-12);
        assert_eq!(r.first.as_ref().map(|d| d.date), Some(days[0].date));
        assert_eq!(r.last.as_ref().map(|d| d.date), Some(days[1].date));
        // No day scored: an empty record, no division by zero.
        let empty = summarize(&[]);
        assert_eq!(empty.days, 0);
        assert_eq!(empty.mean_brier_model, 0.0);
        assert_eq!(score_days(&s, "LAL"), vec![]);
    }

    #[test]
    fn the_latest_day_lists_the_model_favourite_first_and_a_zero_chance_scores_finite() {
        let s = vec![
            snap((2026, 11, 1), "OKC", 0.30, 0.20),
            snap((2026, 11, 2), "BOS", 0.10, 0.25),
            snap((2026, 11, 2), "OKC", 0.40, 0.25),
            snap((2026, 11, 2), "DEN", 0.40, 0.50),
        ];
        let (date, rows) = latest_day(&s).unwrap();
        assert_eq!(date, NaiveDate::from_ymd_opt(2026, 11, 2).unwrap());
        let codes: Vec<&str> = rows.iter().map(|r| r.team_code.as_str()).collect();
        assert_eq!(codes, vec!["DEN", "OKC", "BOS"], "ties by code");
        assert_eq!(days_of(&s).len(), 2);
        assert_eq!(latest_day(&[]), None);
        let zero = vec![snap((2026, 11, 1), "OKC", 0.0, 0.0)];
        let d = score_days(&zero, "OKC");
        assert!(d[0].log_loss_model.is_finite() && d[0].log_loss_model > 10.0);
    }
}
