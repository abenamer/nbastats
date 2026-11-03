//! Dynasty analysis: how unusual is a franchise's title run under season-specific strength?

use serde::{Deserialize, Serialize};

use crate::domain::ids::SeasonYear;
use crate::domain::models::PlayoffResult;
use crate::domain::simulation::stats::{poisson_binomial_pmf, tail_at_least};

pub const MODEL_VERSION: &str = "dynasty-1.0";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DynastySeason {
    pub season: SeasonYear,
    pub code: String,
    pub wins: i32,
    pub losses: i32,
    pub elo_regular_end: Option<f64>,
    pub elo_pre_playoffs: Option<f64>,
    pub made_playoffs: bool,
    pub seed: Option<i32>,
    pub actual_result: PlayoffResult,
    pub champion: bool,
    /// Pre-playoff championship probability from the Elo tournament model (0 if missed).
    pub p_elo: f64,
    /// Equal-strength lottery over every franchise in the league that season.
    pub p_lottery: f64,
    /// Persistent-alpha lottery: alpha / (alpha + N − 1).
    pub p_alpha: f64,
    pub team_count: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelComparison {
    pub name: String,
    pub expected_titles: f64,
    pub p_at_least_actual: f64,
    /// pmf of the title count under this model (index = titles).
    pub distribution: Vec<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DynastyReport {
    pub model_version: String,
    pub franchise_key: String,
    pub franchise_name: String,
    pub from_season: SeasonYear,
    pub to_season: SeasonYear,
    pub alpha: f64,
    pub seasons: Vec<DynastySeason>,
    pub actual_titles: usize,
    pub comparisons: Vec<ModelComparison>,
    /// The franchise's standing in each season's field, and the sum over the window.
    pub strength: Vec<StrengthSeason>,
    pub strength_summary: Option<StrengthSummary>,
}

/// A rated team of a season's field: its code and pre-playoff Elo.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Rated {
    pub code: String,
    pub elo: f64,
}

/// One rated team-season, the input of the strength reading.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FieldRow<'a> {
    pub season: SeasonYear,
    pub franchise_key: &'a str,
    pub code: &'a str,
    pub elo: f64,
}

/// Where the franchise's pre-playoff rating stood in one season's field: against the
/// league mean, the best other team and the worst team, and its rank. The rating fields
/// are `None` for a season the franchise was not rated in.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StrengthSeason {
    pub season: SeasonYear,
    /// Rated teams in the field.
    pub teams: usize,
    pub league_mean: f64,
    pub elo: Option<f64>,
    /// 1 for the strongest.
    pub rank: Option<usize>,
    pub gap_to_mean: Option<f64>,
    /// The strongest team other than the franchise.
    pub best_other: Option<Rated>,
    pub gap_to_best_other: Option<f64>,
    /// The weakest team of the field, the franchise included.
    pub worst: Option<Rated>,
    pub gap_to_worst: Option<f64>,
}

/// The strength seasons summed over the seasons the franchise was rated in.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StrengthSummary {
    /// Seasons of the window with a field, and those the franchise was rated in.
    pub seasons: usize,
    pub seasons_rated: usize,
    pub mean_gap_to_mean: f64,
    pub min_gap_to_mean: f64,
    pub max_gap_to_mean: f64,
    pub mean_gap_to_best_other: f64,
    pub mean_gap_to_worst: f64,
    pub mean_rank: f64,
    pub seasons_first: usize,
}

/// The franchise's standing in each season's field from `from` to `to`: the seasons with at
/// least two rated teams, in order. Ties in rating rank the franchise first.
pub fn strength_seasons(
    key: &str,
    from: SeasonYear,
    to: SeasonYear,
    rows: &[FieldRow],
) -> Vec<StrengthSeason> {
    SeasonYear::range(from, to)
        .filter_map(|season| {
            let mut field: Vec<&FieldRow> = rows.iter().filter(|r| r.season == season).collect();
            if field.len() < 2 {
                return None;
            }
            field.sort_by(|a, b| {
                b.elo
                    .total_cmp(&a.elo)
                    .then((b.franchise_key == key).cmp(&(a.franchise_key == key)))
                    .then(a.code.cmp(b.code))
            });
            let teams = field.len();
            let league_mean = field.iter().map(|r| r.elo).sum::<f64>() / teams as f64;
            let rated = |r: &FieldRow| Rated {
                code: r.code.to_string(),
                elo: r.elo,
            };
            let own = field.iter().position(|r| r.franchise_key == key);
            let elo = own.map(|i| field[i].elo);
            let best_other = field
                .iter()
                .find(|r| r.franchise_key != key)
                .map(|r| rated(r));
            let worst = field.last().map(|r| rated(r));
            Some(StrengthSeason {
                season,
                teams,
                league_mean,
                elo,
                rank: own.map(|i| i + 1),
                gap_to_mean: elo.map(|e| e - league_mean),
                gap_to_best_other: elo.zip(best_other.as_ref()).map(|(e, b)| e - b.elo),
                best_other,
                gap_to_worst: elo.zip(worst.as_ref()).map(|(e, w)| e - w.elo),
                worst,
            })
        })
        .collect()
}

/// Sum the strength seasons; `None` when the franchise was rated in none.
pub fn strength_summary(seasons: &[StrengthSeason]) -> Option<StrengthSummary> {
    let rated: Vec<&StrengthSeason> = seasons.iter().filter(|s| s.elo.is_some()).collect();
    if rated.is_empty() {
        return None;
    }
    let n = rated.len() as f64;
    let mean = |f: &dyn Fn(&StrengthSeason) -> Option<f64>| {
        rated.iter().filter_map(|s| f(s)).sum::<f64>() / n
    };
    let gaps: Vec<f64> = rated.iter().filter_map(|s| s.gap_to_mean).collect();
    Some(StrengthSummary {
        seasons: seasons.len(),
        seasons_rated: rated.len(),
        mean_gap_to_mean: mean(&|s| s.gap_to_mean),
        min_gap_to_mean: gaps.iter().copied().fold(f64::INFINITY, f64::min),
        max_gap_to_mean: gaps.iter().copied().fold(f64::NEG_INFINITY, f64::max),
        mean_gap_to_best_other: mean(&|s| s.gap_to_best_other),
        mean_gap_to_worst: mean(&|s| s.gap_to_worst),
        mean_rank: mean(&|s| s.rank.map(|r| r as f64)),
        seasons_first: rated.iter().filter(|s| s.rank == Some(1)).count(),
    })
}

pub fn compare(seasons: &[DynastySeason], actual: usize) -> Vec<ModelComparison> {
    let make = |name: &str, probs: Vec<f64>| {
        let pmf = poisson_binomial_pmf(&probs);
        ModelComparison {
            name: name.to_string(),
            expected_titles: probs.iter().sum(),
            p_at_least_actual: tail_at_least(&pmf, actual),
            distribution: pmf,
        }
    };
    vec![
        make(
            "equal_lottery",
            seasons.iter().map(|s| s.p_lottery).collect(),
        ),
        make(
            "persistent_alpha",
            seasons.iter().map(|s| s.p_alpha).collect(),
        ),
        make("historical_elo", seasons.iter().map(|s| s.p_elo).collect()),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_abs_diff_eq;

    #[test]
    fn comparisons_are_consistent() {
        let seasons: Vec<DynastySeason> = (0..4)
            .map(|i| DynastySeason {
                season: SeasonYear::of(2015 + i),
                code: "GSW".into(),
                wins: 60,
                losses: 22,
                elo_regular_end: None,
                elo_pre_playoffs: None,
                made_playoffs: true,
                seed: Some(1),
                actual_result: PlayoffResult::Champion,
                champion: true,
                p_elo: 0.5,
                p_lottery: 1.0 / 30.0,
                p_alpha: 0.1,
                team_count: 30,
            })
            .collect();
        let c = compare(&seasons, 4);
        assert_abs_diff_eq!(
            c[0].p_at_least_actual,
            (1.0f64 / 30.0).powi(4),
            epsilon = 1e-12
        );
        assert_abs_diff_eq!(c[2].expected_titles, 2.0, epsilon = 1e-12);
        assert_abs_diff_eq!(c[2].p_at_least_actual, 0.0625, epsilon = 1e-12);
    }

    #[test]
    fn the_strength_seasons_rank_the_franchise_in_each_field() {
        let row = |season: i32, key: &'static str, elo: f64| FieldRow {
            season: SeasonYear::of(season),
            franchise_key: key,
            code: key,
            elo,
        };
        let rows = [
            // 2015: GSW first by 40 over SAS; CHA last. 2016: GSW tied with CLE, ranked
            // first by the tie rule, and the worst is 300 below. 2017: GSW unrated.
            row(2015, "GSW", 1700.0),
            row(2015, "SAS", 1660.0),
            row(2015, "CHA", 1400.0),
            row(2016, "CLE", 1650.0),
            row(2016, "GSW", 1650.0),
            row(2016, "PHI", 1350.0),
            row(2017, "CLE", 1600.0),
            row(2017, "BOS", 1500.0),
            // 2018 has one rated team: no field.
            row(2018, "GSW", 1700.0),
        ];
        let s = strength_seasons("GSW", SeasonYear::of(2015), SeasonYear::of(2018), &rows);
        assert_eq!(s.len(), 3);
        let first = &s[0];
        assert_eq!((first.teams, first.rank), (3, Some(1)));
        assert_abs_diff_eq!(first.league_mean, 4760.0 / 3.0, epsilon = 1e-9);
        assert_abs_diff_eq!(
            first.gap_to_mean.unwrap(),
            113.333_333_333_333_3,
            epsilon = 1e-9
        );
        assert_eq!(first.best_other.as_ref().unwrap().code, "SAS");
        assert_abs_diff_eq!(first.gap_to_best_other.unwrap(), 40.0, epsilon = 1e-12);
        assert_eq!(first.worst.as_ref().unwrap().code, "CHA");
        assert_abs_diff_eq!(first.gap_to_worst.unwrap(), 300.0, epsilon = 1e-12);
        let tied = &s[1];
        assert_eq!(tied.rank, Some(1));
        assert_eq!(tied.best_other.as_ref().unwrap().code, "CLE");
        assert_abs_diff_eq!(tied.gap_to_best_other.unwrap(), 0.0, epsilon = 1e-12);
        let unrated = &s[2];
        assert_eq!((unrated.elo, unrated.rank, unrated.teams), (None, None, 2));
        assert_eq!(unrated.best_other.as_ref().unwrap().code, "CLE");
        let sum = strength_summary(&s).unwrap();
        assert_eq!(
            (sum.seasons, sum.seasons_rated, sum.seasons_first),
            (3, 2, 2)
        );
        assert_abs_diff_eq!(sum.mean_gap_to_best_other, 20.0, epsilon = 1e-12);
        assert_abs_diff_eq!(sum.mean_rank, 1.0, epsilon = 1e-12);
        assert_abs_diff_eq!(sum.min_gap_to_mean, 100.0, epsilon = 1e-9);
        assert!(sum.max_gap_to_mean > sum.min_gap_to_mean);
        // A franchise rated in no season of the window has no summary.
        assert!(strength_summary(&strength_seasons(
            "BOS",
            SeasonYear::of(2015),
            SeasonYear::of(2016),
            &rows
        ))
        .is_none());
    }
}
