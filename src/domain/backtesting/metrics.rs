//! Proper scoring rules and calibration helpers.

use serde::{Deserialize, Serialize};

use crate::domain::models::Round;
use crate::domain::simulation::stats::wilson_interval;

/// Multi-class Brier score: Σ (p_i − y_i)² over the candidate set.
pub fn multiclass_brier(probs: &[f64], outcomes: &[bool]) -> f64 {
    probs
        .iter()
        .zip(outcomes)
        .map(|(p, &y)| (p - if y { 1.0 } else { 0.0 }).powi(2))
        .sum()
}

/// −ln p of the realised outcome, floored to avoid infinities.
pub fn log_loss(p_actual: f64) -> f64 {
    -(p_actual.max(1e-9)).ln()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Summary {
    pub seasons: usize,
    pub mean_brier: f64,
    pub brier_se: f64,
    pub mean_log_loss: f64,
    pub log_loss_se: f64,
    pub top_pick_accuracy: f64,
    pub mean_p_winner: f64,
    /// 1 − Brier / Brier of a uniform 1/16 forecast.
    pub brier_skill_vs_equal: f64,
}

pub fn summarise(seasons: &[super::SeasonScore]) -> Summary {
    let n = seasons.len().max(1) as f64;
    let mean = |f: &dyn Fn(&super::SeasonScore) -> f64| seasons.iter().map(f).sum::<f64>() / n;
    let se = |f: &dyn Fn(&super::SeasonScore) -> f64| {
        if seasons.len() < 2 {
            return 0.0;
        }
        let m = mean(f);
        let var = seasons.iter().map(|s| (f(s) - m).powi(2)).sum::<f64>() / (n - 1.0);
        (var / n).sqrt()
    };
    let mean_brier = mean(&|s| s.brier);
    // uniform forecast over 16 teams: 15*(1/16)^2 + (15/16)^2 = 0.9375
    let equal_brier = 15.0 * (1.0f64 / 16.0).powi(2) + (15.0f64 / 16.0).powi(2);
    Summary {
        seasons: seasons.len(),
        mean_brier,
        brier_se: se(&|s| s.brier),
        mean_log_loss: mean(&|s| s.log_loss),
        log_loss_se: se(&|s| s.log_loss),
        top_pick_accuracy: mean(&|s| if s.hit { 1.0 } else { 0.0 }),
        mean_p_winner: mean(&|s| s.p_champion),
        brier_skill_vs_equal: 1.0 - mean_brier / equal_brier,
    }
}

/// Scores of the played series: one trial per series, so a season contributes fifteen trials
/// instead of one champion.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SeriesSummary {
    pub series: usize,
    pub mean_brier: f64,
    pub brier_se: f64,
    pub mean_log_loss: f64,
    pub log_loss_se: f64,
    /// Share of the series with a favourite that the favourite won.
    pub favourite_won: f64,
    /// Series in which the model had a favourite (a probability other than one half).
    pub with_favourite: usize,
    /// The favourite's mean probability over the series with a favourite.
    pub mean_p_favourite: f64,
    /// 1 − Brier / Brier of the equal model, a coin flip (0.25); 0 without series.
    pub brier_skill_vs_equal: f64,
    pub by_round: Vec<RoundSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoundSummary {
    pub round: Round,
    pub series: usize,
    pub mean_brier: f64,
    pub mean_log_loss: f64,
    pub favourite_won: f64,
    pub with_favourite: usize,
    /// The favourite's mean probability over the round's series with a favourite.
    pub mean_p_favourite: f64,
}

/// A model's series scores against another model's on the same series: the mean of the
/// differences (this model minus the other, so a negative value favours this model) with
/// its standard error over the series. The pairing removes the variation the series share.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PairedDifference {
    pub against: String,
    pub series: usize,
    pub log_loss: f64,
    pub log_loss_se: f64,
    pub brier: f64,
    pub brier_se: f64,
}

/// The paired difference of `this` against `other` over the series both scored, matched by
/// season, round and participants; `None` with fewer than two series in common.
pub fn paired_difference(
    against: &str,
    this: &[super::SeriesScore],
    other: &[super::SeriesScore],
) -> Option<PairedDifference> {
    let key = |s: &super::SeriesScore| (s.season, s.round, s.winner_id, s.loser_id);
    let others: std::collections::HashMap<_, &super::SeriesScore> =
        other.iter().map(|s| (key(s), s)).collect();
    let pairs: Vec<(&super::SeriesScore, &super::SeriesScore)> = this
        .iter()
        .filter_map(|s| others.get(&key(s)).map(|o| (s, *o)))
        .collect();
    if pairs.len() < 2 {
        return None;
    }
    let n = pairs.len() as f64;
    let stat = |f: &dyn Fn(&super::SeriesScore) -> f64| {
        let diffs: Vec<f64> = pairs.iter().map(|(a, b)| f(a) - f(b)).collect();
        let mean = diffs.iter().sum::<f64>() / n;
        let var = diffs.iter().map(|d| (d - mean).powi(2)).sum::<f64>() / (n - 1.0);
        (mean, (var / n).sqrt())
    };
    let (log_loss, log_loss_se) = stat(&|s| s.log_loss);
    let (brier, brier_se) = stat(&|s| s.brier);
    Some(PairedDifference {
        against: against.to_string(),
        series: pairs.len(),
        log_loss,
        log_loss_se,
        brier,
        brier_se,
    })
}

/// Brier score of a coin flip on a two-sided outcome.
pub const COIN_FLIP_BRIER: f64 = 0.25;

/// The favourite's share of wins, the count of series with a favourite and the favourite's
/// mean probability over them.
fn favourite_share(scores: &[&super::SeriesScore]) -> (f64, usize, f64) {
    let with: Vec<&super::SeriesScore> =
        scores.iter().copied().filter(|s| s.hit.is_some()).collect();
    if with.is_empty() {
        return (0.0, 0, 0.0);
    }
    let n = with.len() as f64;
    let won = with.iter().filter(|s| s.hit == Some(true)).count() as f64;
    let p = with
        .iter()
        .map(|s| s.p_winner.max(1.0 - s.p_winner))
        .sum::<f64>()
        / n;
    (won / n, with.len(), p)
}

pub fn summarise_series(scores: &[super::SeriesScore]) -> SeriesSummary {
    let n = scores.len().max(1) as f64;
    let mean = |f: &dyn Fn(&super::SeriesScore) -> f64| scores.iter().map(f).sum::<f64>() / n;
    let se = |f: &dyn Fn(&super::SeriesScore) -> f64| {
        if scores.len() < 2 {
            return 0.0;
        }
        let m = mean(f);
        let var = scores.iter().map(|s| (f(s) - m).powi(2)).sum::<f64>() / (n - 1.0);
        (var / n).sqrt()
    };
    let all: Vec<&super::SeriesScore> = scores.iter().collect();
    let (favourite_won, with_favourite, mean_p_favourite) = favourite_share(&all);
    let mean_brier = mean(&|s| s.brier);
    let by_round = Round::ALL
        .into_iter()
        .map(|round| {
            let of_round: Vec<&super::SeriesScore> =
                scores.iter().filter(|s| s.round == round).collect();
            let k = of_round.len().max(1) as f64;
            let (favourite_won, with_favourite, mean_p_favourite) = favourite_share(&of_round);
            RoundSummary {
                round,
                series: of_round.len(),
                mean_brier: of_round.iter().map(|s| s.brier).sum::<f64>() / k,
                mean_log_loss: of_round.iter().map(|s| s.log_loss).sum::<f64>() / k,
                favourite_won,
                with_favourite,
                mean_p_favourite,
            }
        })
        .collect();
    SeriesSummary {
        series: scores.len(),
        mean_brier,
        brier_se: se(&|s| s.brier),
        mean_log_loss: mean(&|s| s.log_loss),
        log_loss_se: se(&|s| s.log_loss),
        favourite_won,
        with_favourite,
        mean_p_favourite,
        brier_skill_vs_equal: if scores.is_empty() {
            0.0
        } else {
            1.0 - mean_brier / COIN_FLIP_BRIER
        },
        by_round,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalibrationBin {
    pub lower: f64,
    pub upper: f64,
    pub count: usize,
    pub mean_predicted: f64,
    pub observed: f64,
    pub observed_ci95: (f64, f64),
}

const EDGES: [f64; 8] = [0.0, 0.05, 0.1, 0.2, 0.3, 0.5, 0.7, 1.0001];
/// Bins for the favourite's series probability, which is at least one half.
pub const SERIES_EDGES: [f64; 8] = [0.5, 0.55, 0.6, 0.65, 0.7, 0.8, 0.9, 1.0001];

pub fn calibration(pairs: &[(f64, bool)]) -> Vec<CalibrationBin> {
    calibration_with(pairs, &EDGES)
}

pub fn calibration_with(pairs: &[(f64, bool)], edges: &[f64]) -> Vec<CalibrationBin> {
    edges
        .windows(2)
        .map(|w| {
            let (lo, hi) = (w[0], w[1]);
            let in_bin: Vec<&(f64, bool)> =
                pairs.iter().filter(|(p, _)| *p >= lo && *p < hi).collect();
            let count = in_bin.len();
            let hits = in_bin.iter().filter(|(_, y)| *y).count() as f64;
            let mean_predicted = if count > 0 {
                in_bin.iter().map(|(p, _)| p).sum::<f64>() / count as f64
            } else {
                0.0
            };
            CalibrationBin {
                lower: lo,
                upper: hi.min(1.0),
                count,
                mean_predicted,
                observed: if count > 0 { hits / count as f64 } else { 0.0 },
                observed_ci95: wilson_interval(hits, count as f64),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_abs_diff_eq;

    #[test]
    fn brier_of_perfect_and_uniform_forecasts() {
        let mut outcomes = vec![false; 16];
        outcomes[0] = true;
        let mut perfect = vec![0.0; 16];
        perfect[0] = 1.0;
        assert_abs_diff_eq!(multiclass_brier(&perfect, &outcomes), 0.0, epsilon = 1e-12);
        assert_abs_diff_eq!(
            multiclass_brier(&[1.0 / 16.0; 16], &outcomes),
            0.9375,
            epsilon = 1e-12
        );
    }

    #[test]
    fn calibration_bins_count_everything() {
        let pairs: Vec<(f64, bool)> = (0..100).map(|i| (i as f64 / 100.0, i % 2 == 0)).collect();
        let bins = calibration(&pairs);
        assert_eq!(bins.iter().map(|b| b.count).sum::<usize>(), 100);
    }
}
