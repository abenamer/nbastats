//! Engine A: alpha-weighted championship lottery.
//!
//! Each season exactly one franchise wins the title; franchise `i` wins with probability
//! `alpha_i / Σ_j alpha_j`. Seasons are independent. Alphas can be constant or vary by
//! season (a full `seasons × franchises` schedule), which is how the dynasty laboratory feeds
//! historical, season-specific Elo-derived championship probabilities into the same engine.

use rand::Rng;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use super::exact;
use super::progress::{ChunkTracker, Progress};
use super::rng::{chunk_rng, chunks};
use super::stats;

pub const MODEL_VERSION: &str = "alpha-lottery-1.0";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlphaParams {
    /// Franchise labels (length = number of franchises).
    pub franchises: Vec<String>,
    /// Constant alpha per franchise (used when `schedule` is None).
    pub alphas: Vec<f64>,
    /// Optional season-dependent alphas: `schedule[season][franchise]`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schedule: Option<Vec<Vec<f64>>>,
    pub seasons: usize,
    pub iterations: u64,
    /// Championship threshold K.
    pub threshold: usize,
    pub seed: u64,
    /// Index of the franchise of interest for "P(specific franchise wins >= K)".
    #[serde(default)]
    pub target_index: usize,
}

impl AlphaParams {
    pub fn equal(n: usize, seasons: usize, iterations: u64, threshold: usize, seed: u64) -> Self {
        Self {
            franchises: (1..=n).map(|i| format!("Franchise {i}")).collect(),
            alphas: vec![1.0; n],
            schedule: None,
            seasons,
            iterations,
            threshold,
            seed,
            target_index: 0,
        }
    }

    pub fn validate(&self, max_iterations: u64) -> Result<(), String> {
        let n = self.franchises.len();
        if n == 0 || n > 500 {
            return Err("number of franchises must be between 1 and 500".into());
        }
        if self.alphas.len() != n {
            return Err("alphas must have one entry per franchise".into());
        }
        if self.alphas.iter().any(|a| !a.is_finite() || *a < 0.0) {
            return Err("alphas must be finite and non-negative".into());
        }
        if self.alphas.iter().sum::<f64>() <= 0.0 {
            return Err("at least one alpha must be positive".into());
        }
        if self.seasons == 0 || self.seasons > 500 {
            return Err("seasons must be between 1 and 500".into());
        }
        if self.iterations == 0 || self.iterations > max_iterations {
            return Err(format!("iterations must be between 1 and {max_iterations}"));
        }
        if self.threshold == 0 || self.threshold > self.seasons {
            return Err("threshold K must be between 1 and the number of seasons".into());
        }
        if self.target_index >= n {
            return Err("target franchise index out of range".into());
        }
        if let Some(s) = &self.schedule {
            if s.len() != self.seasons {
                return Err("schedule must have one row per season".into());
            }
            for row in s {
                if row.len() != n {
                    return Err("each schedule row must have one alpha per franchise".into());
                }
                if row.iter().any(|a| !a.is_finite() || *a < 0.0) || row.iter().sum::<f64>() <= 0.0 {
                    return Err("schedule rows must be non-negative with a positive sum".into());
                }
            }
        }
        Ok(())
    }

    /// Per-season probability vectors (normalised).
    pub fn season_probabilities(&self) -> Vec<Vec<f64>> {
        match &self.schedule {
            Some(s) => s.iter().map(|row| normalise(row)).collect(),
            None => vec![normalise(&self.alphas); self.seasons],
        }
    }
}

fn normalise(v: &[f64]) -> Vec<f64> {
    let total: f64 = v.iter().sum();
    v.iter().map(|x| x / total).collect()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FranchiseOutcome {
    pub name: String,
    pub alpha: f64,
    /// Mean per-season championship probability (exact).
    pub probability: f64,
    pub expected_titles: f64,
    pub simulated_mean_titles: f64,
    pub p_at_least_k: f64,
    pub p_at_least_k_se: f64,
    pub p_at_least_k_exact: Option<f64>,
    /// Distribution of this franchise's title count (index = titles).
    pub title_distribution: Vec<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlphaResult {
    pub model_version: String,
    pub iterations: u64,
    pub seasons: usize,
    pub threshold: usize,
    pub seed: u64,
    pub p_any_at_least_k: f64,
    pub p_any_at_least_k_se: f64,
    pub p_any_at_least_k_ci95: (f64, f64),
    /// Exact value (only when alphas are constant across seasons).
    pub p_any_at_least_k_exact: Option<f64>,
    pub p_target_at_least_k: f64,
    pub p_target_at_least_k_se: f64,
    pub p_target_at_least_k_exact: Option<f64>,
    /// P(max titles of any franchise = j), index j.
    pub max_titles_distribution: Vec<f64>,
    pub max_titles_distribution_exact: Option<Vec<f64>>,
    /// P(number of franchises with >= K titles = j), index j.
    pub count_at_least_k_distribution: Vec<f64>,
    pub expected_count_at_least_k: f64,
    pub top5_share_mean: f64,
    pub gini_mean: f64,
    pub hhi_mean: f64,
    /// Distribution of titles-per-franchise aggregated over franchises:
    /// expected number of franchises with exactly j titles.
    pub franchises_by_title_count: Vec<f64>,
    pub franchises: Vec<FranchiseOutcome>,
    pub elapsed_ms: u128,
}

#[derive(Default, Clone)]
struct Accum {
    iterations: u64,
    any_at_least_k: u64,
    target_at_least_k: u64,
    max_hist: Vec<u64>,
    count_hist: Vec<u64>,
    per_franchise_hist: Vec<Vec<u64>>, // [franchise][titles]
    title_sum: Vec<f64>,
    top5_sum: f64,
    gini_sum: f64,
    hhi_sum: f64,
}

impl Accum {
    fn new(n: usize, seasons: usize) -> Self {
        Self {
            max_hist: vec![0; seasons + 1],
            count_hist: vec![0; n + 1],
            per_franchise_hist: vec![vec![0; seasons + 1]; n],
            title_sum: vec![0.0; n],
            ..Default::default()
        }
    }
    fn merge(mut self, other: Accum) -> Accum {
        self.iterations += other.iterations;
        self.any_at_least_k += other.any_at_least_k;
        self.target_at_least_k += other.target_at_least_k;
        for (a, b) in self.max_hist.iter_mut().zip(&other.max_hist) {
            *a += b;
        }
        for (a, b) in self.count_hist.iter_mut().zip(&other.count_hist) {
            *a += b;
        }
        for (ha, hb) in self.per_franchise_hist.iter_mut().zip(&other.per_franchise_hist) {
            for (a, b) in ha.iter_mut().zip(hb) {
                *a += b;
            }
        }
        for (a, b) in self.title_sum.iter_mut().zip(&other.title_sum) {
            *a += b;
        }
        self.top5_sum += other.top5_sum;
        self.gini_sum += other.gini_sum;
        self.hhi_sum += other.hhi_sum;
        self
    }
}

/// Cumulative distribution tables per season for O(log n) sampling.
fn cumulative(probs: &[Vec<f64>]) -> Vec<Vec<f64>> {
    probs
        .iter()
        .map(|row| {
            let mut acc = 0.0;
            let mut c: Vec<f64> = row
                .iter()
                .map(|p| {
                    acc += p;
                    acc
                })
                .collect();
            if let Some(last) = c.last_mut() {
                *last = 1.0;
            }
            c
        })
        .collect()
}

fn sample(cdf: &[f64], u: f64) -> usize {
    match cdf.binary_search_by(|c| c.partial_cmp(&u).unwrap()) {
        Ok(i) => (i + 1).min(cdf.len() - 1),
        Err(i) => i.min(cdf.len() - 1),
    }
}

/// Run the simulation. `progress` receives a fraction in [0,1] after each chunk and may
/// request cancellation (the partial result is still returned).
pub fn run<P: Progress + ?Sized>(params: &AlphaParams, progress: &P) -> AlphaResult {
    let start = std::time::Instant::now();
    let n = params.franchises.len();
    let seasons = params.seasons;
    let k = params.threshold;
    let probs = params.season_probabilities();
    let cdfs = cumulative(&probs);
    let chunk_list = chunks(params.iterations, 5_000);
    let tracker = ChunkTracker::new(progress, chunk_list.len() as u64);

    let accum = chunk_list
        .par_iter()
        .map(|&(chunk_idx, count)| {
            let mut acc = Accum::new(n, seasons);
            if tracker.is_cancelled() {
                return acc;
            }
            let mut rng = chunk_rng(params.seed, chunk_idx);
            let mut titles = vec![0u32; n];
            let mut titles_f = vec![0f64; n];
            for _ in 0..count {
                titles.iter_mut().for_each(|t| *t = 0);
                for cdf in &cdfs {
                    let u: f64 = rng.gen();
                    titles[sample(cdf, u)] += 1;
                }
                let mut max = 0u32;
                let mut cnt = 0usize;
                for (i, &t) in titles.iter().enumerate() {
                    max = max.max(t);
                    if t as usize >= k {
                        cnt += 1;
                    }
                    acc.per_franchise_hist[i][t as usize] += 1;
                    acc.title_sum[i] += t as f64;
                    titles_f[i] = t as f64;
                }
                if cnt > 0 {
                    acc.any_at_least_k += 1;
                }
                if titles[params.target_index] as usize >= k {
                    acc.target_at_least_k += 1;
                }
                acc.max_hist[max as usize] += 1;
                acc.count_hist[cnt] += 1;
                acc.top5_sum += stats::top_k_share(&titles_f, 5);
                acc.gini_sum += stats::gini(&titles_f);
                acc.hhi_sum += stats::hhi(&titles_f);
                acc.iterations += 1;
            }
            tracker.chunk_done();
            acc
        })
        .reduce(|| Accum::new(n, seasons), Accum::merge);

    let it = accum.iterations.max(1) as f64;
    let p_any = accum.any_at_least_k as f64 / it;
    let p_target = accum.target_at_least_k as f64 / it;
    let constant = params.schedule.is_none();
    let base_probs = &probs[0];
    let exact_any = constant.then(|| exact::p_any_at_least(base_probs, seasons, k));
    let exact_max = constant.then(|| exact::max_titles_distribution(base_probs, seasons));
    let se_any = stats::binomial_se(p_any, accum.iterations);

    let mut franchises_by_title_count = vec![0.0; seasons + 1];
    let franchises: Vec<FranchiseOutcome> = (0..n)
        .map(|i| {
            let p_mean = probs.iter().map(|row| row[i]).sum::<f64>() / seasons as f64;
            let hist: Vec<f64> = accum.per_franchise_hist[i].iter().map(|&c| c as f64 / it).collect();
            for (j, h) in hist.iter().enumerate() {
                franchises_by_title_count[j] += h;
            }
            let p_k: f64 = hist.iter().skip(k).sum();
            FranchiseOutcome {
                name: params.franchises[i].clone(),
                alpha: params.alphas.get(i).copied().unwrap_or(1.0),
                probability: p_mean,
                expected_titles: probs.iter().map(|row| row[i]).sum(),
                simulated_mean_titles: accum.title_sum[i] / it,
                p_at_least_k: p_k,
                p_at_least_k_se: stats::binomial_se(p_k, accum.iterations),
                p_at_least_k_exact: constant.then(|| exact::p_franchise_at_least(base_probs[i], seasons, k)),
                title_distribution: hist,
            }
        })
        .collect();

    AlphaResult {
        model_version: MODEL_VERSION.to_string(),
        iterations: accum.iterations,
        seasons,
        threshold: k,
        seed: params.seed,
        p_any_at_least_k: p_any,
        p_any_at_least_k_se: se_any,
        p_any_at_least_k_ci95: ((p_any - 1.96 * se_any).max(0.0), (p_any + 1.96 * se_any).min(1.0)),
        p_any_at_least_k_exact: exact_any,
        p_target_at_least_k: p_target,
        p_target_at_least_k_se: stats::binomial_se(p_target, accum.iterations),
        p_target_at_least_k_exact: constant.then(|| exact::p_franchise_at_least(base_probs[params.target_index], seasons, k)),
        max_titles_distribution: accum.max_hist.iter().map(|&c| c as f64 / it).collect(),
        max_titles_distribution_exact: exact_max,
        count_at_least_k_distribution: accum.count_hist.iter().map(|&c| c as f64 / it).collect(),
        expected_count_at_least_k: accum
            .count_hist
            .iter()
            .enumerate()
            .map(|(j, &c)| j as f64 * c as f64)
            .sum::<f64>()
            / it,
        top5_share_mean: accum.top5_sum / it,
        gini_mean: accum.gini_sum / it,
        hhi_mean: accum.hhi_sum / it,
        franchises_by_title_count,
        franchises,
        elapsed_ms: start.elapsed().as_millis(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::simulation::progress::Silent;
    use approx::assert_abs_diff_eq;

    #[test]
    fn equal_strength_normalises() {
        let p = AlphaParams::equal(30, 22, 10, 4, 1);
        let probs = p.season_probabilities();
        assert_eq!(probs.len(), 22);
        for row in &probs {
            assert_abs_diff_eq!(row.iter().sum::<f64>(), 1.0, epsilon = 1e-12);
            assert_abs_diff_eq!(row[0], 1.0 / 30.0, epsilon = 1e-12);
        }
    }

    #[test]
    fn unequal_alpha_normalises() {
        let mut p = AlphaParams::equal(3, 1, 10, 1, 1);
        p.alphas = vec![2.0, 1.0, 1.0];
        let probs = p.season_probabilities();
        assert_abs_diff_eq!(probs[0][0], 0.5, epsilon = 1e-12);
    }

    #[test]
    fn converges_to_exact_occupancy_probability() {
        let p = AlphaParams::equal(30, 22, 200_000, 4, 42);
        let r = run(&p, &Silent);
        let exact = r.p_any_at_least_k_exact.unwrap();
        assert_abs_diff_eq!(exact, 0.1605, epsilon = 0.0006);
        // within 4 standard errors
        assert!((r.p_any_at_least_k - exact).abs() < 4.0 * r.p_any_at_least_k_se, "{} vs {}", r.p_any_at_least_k, exact);
        assert_abs_diff_eq!(r.max_titles_distribution.iter().sum::<f64>(), 1.0, epsilon = 1e-9);
        assert_abs_diff_eq!(r.franchises[0].expected_titles, 22.0 / 30.0, epsilon = 1e-12);
        assert!((r.franchises[0].simulated_mean_titles - 22.0 / 30.0).abs() < 0.02);
    }

    #[test]
    fn same_seed_same_result() {
        let p = AlphaParams::equal(10, 10, 20_000, 3, 7);
        let a = run(&p, &Silent);
        let b = run(&p, &Silent);
        assert_eq!(a.p_any_at_least_k, b.p_any_at_least_k);
        assert_eq!(a.max_titles_distribution, b.max_titles_distribution);
        let mut p2 = p.clone();
        p2.seed = 8;
        let c = run(&p2, &Silent);
        assert_ne!(a.max_titles_distribution, c.max_titles_distribution);
    }

    #[test]
    fn dominant_franchise_wins_everything() {
        let mut p = AlphaParams::equal(5, 6, 1000, 6, 1);
        p.alphas = vec![1.0, 0.0, 0.0, 0.0, 0.0];
        let r = run(&p, &Silent);
        assert_abs_diff_eq!(r.p_any_at_least_k, 1.0, epsilon = 1e-12);
        assert_abs_diff_eq!(r.p_target_at_least_k, 1.0, epsilon = 1e-12);
        assert_abs_diff_eq!(r.gini_mean, 0.8, epsilon = 1e-12);
        assert_abs_diff_eq!(r.hhi_mean, 1.0, epsilon = 1e-12);
    }

    #[test]
    fn schedule_mode_runs() {
        let mut p = AlphaParams::equal(3, 2, 5000, 1, 3);
        p.schedule = Some(vec![vec![1.0, 0.0, 0.0], vec![0.0, 1.0, 0.0]]);
        assert!(p.validate(1_000_000).is_ok());
        let r = run(&p, &Silent);
        assert!(r.p_any_at_least_k_exact.is_none());
        assert_abs_diff_eq!(r.franchises[0].expected_titles, 1.0, epsilon = 1e-12);
        assert_abs_diff_eq!(r.franchises[2].simulated_mean_titles, 0.0, epsilon = 1e-12);
    }

    #[test]
    fn validation_rejects_bad_input() {
        let mut p = AlphaParams::equal(30, 22, 10, 4, 1);
        p.threshold = 0;
        assert!(p.validate(100).is_err());
        p.threshold = 4;
        p.iterations = 1000;
        assert!(p.validate(100).is_err());
        p.iterations = 10;
        p.alphas[0] = -1.0;
        assert!(p.validate(100).is_err());
    }
}
