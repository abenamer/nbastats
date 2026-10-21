//! Dynasty analysis: how unusual is a franchise's title run under season-specific strength?

use serde::{Deserialize, Serialize};

use crate::models::PlayoffResult;
use crate::simulation::stats::{poisson_binomial_pmf, tail_at_least};

pub const MODEL_VERSION: &str = "dynasty-1.0";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DynastySeason {
    pub season: i32,
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
    pub from_season: i32,
    pub to_season: i32,
    pub alpha: f64,
    pub seasons: Vec<DynastySeason>,
    pub actual_titles: usize,
    pub comparisons: Vec<ModelComparison>,
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
        make("equal_lottery", seasons.iter().map(|s| s.p_lottery).collect()),
        make("persistent_alpha", seasons.iter().map(|s| s.p_alpha).collect()),
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
                season: 2015 + i,
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
        assert_abs_diff_eq!(c[0].p_at_least_actual, (1.0f64 / 30.0).powi(4), epsilon = 1e-12);
        assert_abs_diff_eq!(c[2].expected_titles, 2.0, epsilon = 1e-12);
        assert_abs_diff_eq!(c[2].p_at_least_actual, 0.0625, epsilon = 1e-12);
    }
}
