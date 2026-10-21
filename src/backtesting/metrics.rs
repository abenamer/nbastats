//! Proper scoring rules and calibration helpers.

use serde::{Deserialize, Serialize};

use crate::simulation::stats::wilson_interval;

/// Multi-class Brier score: Σ (p_i − y_i)² over the candidate set.
pub fn multiclass_brier(probs: &[f64], outcomes: &[bool]) -> f64 {
    probs.iter().zip(outcomes).map(|(p, &y)| (p - if y { 1.0 } else { 0.0 }).powi(2)).sum()
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

pub fn calibration(pairs: &[(f64, bool)]) -> Vec<CalibrationBin> {
    EDGES
        .windows(2)
        .map(|w| {
            let (lo, hi) = (w[0], w[1]);
            let in_bin: Vec<&(f64, bool)> = pairs.iter().filter(|(p, _)| *p >= lo && *p < hi).collect();
            let count = in_bin.len();
            let hits = in_bin.iter().filter(|(_, y)| *y).count() as f64;
            let mean_predicted = if count > 0 { in_bin.iter().map(|(p, _)| p).sum::<f64>() / count as f64 } else { 0.0 };
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
        assert_abs_diff_eq!(multiclass_brier(&[1.0 / 16.0; 16], &outcomes), 0.9375, epsilon = 1e-12);
    }

    #[test]
    fn calibration_bins_count_everything() {
        let pairs: Vec<(f64, bool)> = (0..100).map(|i| (i as f64 / 100.0, i % 2 == 0)).collect();
        let bins = calibration(&pairs);
        assert_eq!(bins.iter().map(|b| b.count).sum::<usize>(), 100);
    }
}
