//! Exact championship-occupancy probabilities for constant franchise strengths.
//!
//! With `n` independent seasons and franchise win probabilities `p_i`, the probability that
//! every franchise wins fewer than `k` titles is
//!
//!   n! · [x^n] ∏_i ( Σ_{j<k} (p_i x)^j / j! )
//!
//! which we evaluate with a polynomial product in O(m·n·k). This validates the Monte Carlo
//! engine: for 30 equal franchises, 22 seasons and k=4 the answer is ≈ 0.1605.

/// Probability that at least one franchise wins at least `k` titles.
pub fn p_any_at_least(probs: &[f64], seasons: usize, k: usize) -> f64 {
    1.0 - p_all_below(probs, seasons, k)
}

/// Probability that every franchise wins fewer than `k` titles over `seasons` seasons.
pub fn p_all_below(probs: &[f64], seasons: usize, k: usize) -> f64 {
    if k == 0 {
        return 0.0;
    }
    let total: f64 = probs.iter().sum();
    if total <= 0.0 {
        return 1.0;
    }
    // Work with scaled polynomials: coefficient of x^j is p^j / j!, truncated to degree < k.
    // Product truncated to degree <= seasons.
    let mut poly = vec![0.0f64; seasons + 1];
    poly[0] = 1.0;
    let factorials = factorials(seasons.max(k));
    for &p_raw in probs {
        let p = p_raw / total;
        let mut term = vec![0.0f64; k.min(seasons + 1)];
        for (j, t) in term.iter_mut().enumerate() {
            *t = p.powi(j as i32) / factorials[j];
        }
        let mut next = vec![0.0f64; seasons + 1];
        for (a, &ca) in poly.iter().enumerate() {
            if ca == 0.0 {
                continue;
            }
            for (b, &cb) in term.iter().enumerate() {
                if a + b > seasons {
                    break;
                }
                next[a + b] += ca * cb;
            }
        }
        poly = next;
    }
    (poly[seasons] * factorials[seasons]).clamp(0.0, 1.0)
}

/// Probability that a specific franchise with probability `p` wins at least `k` of `n` titles
/// (binomial tail).
pub fn p_franchise_at_least(p: f64, n: usize, k: usize) -> f64 {
    let mut total = 0.0;
    for j in k..=n {
        total += binomial(n, j) * p.powi(j as i32) * (1.0 - p).powi((n - j) as i32);
    }
    total.clamp(0.0, 1.0)
}

/// Exact distribution of the maximum title count across franchises: P(max = j) for j in 0..=n.
pub fn max_titles_distribution(probs: &[f64], seasons: usize) -> Vec<f64> {
    // P(max = j) = P(all < j+1) - P(all < j)
    let mut cdf = Vec::with_capacity(seasons + 2);
    for k in 0..=seasons + 1 {
        cdf.push(p_all_below(probs, seasons, k));
    }
    (0..=seasons)
        .map(|j| (cdf[j + 1] - cdf[j]).max(0.0))
        .collect()
}

/// How strong a franchise had to be to win `titles` of `seasons` titles among `franchises`
/// franchises with every other one at alpha 1: the record read backwards.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Strength {
    pub franchises: usize,
    pub seasons: usize,
    pub titles: usize,
    /// The title chance per season that makes the titles the expected count
    /// (titles divided by seasons), which is also the maximum-likelihood chance.
    pub probability: f64,
    /// The alpha with that chance against the others at 1; `None` when the titles equal the
    /// seasons, where no finite alpha gives a sure title every season.
    pub alpha: Option<f64>,
    /// Wilson 95 percent interval of the title chance, and the alphas at its ends.
    pub probability_low: f64,
    pub probability_high: f64,
    pub alpha_low: Option<f64>,
    pub alpha_high: Option<f64>,
    /// The chance of at least `titles` titles at the implied strength.
    pub p_at_least: f64,
    /// The chance of at least `titles` titles for an ordinary franchise (alpha 1).
    pub p_at_least_equal: f64,
    /// The title chance and alpha at which `titles` or more becomes a coin flip.
    pub even_odds_probability: f64,
    pub even_odds_alpha: Option<f64>,
}

/// The alpha that gives title chance `p` when every other of `franchises` franchises is at 1.
fn alpha_for(p: f64, franchises: usize) -> Option<f64> {
    (p < 1.0).then(|| (franchises - 1) as f64 * p / (1.0 - p))
}

/// Wilson 95 percent interval of a proportion `k` of `n`.
fn wilson(k: usize, n: usize) -> (f64, f64) {
    let z = 1.96_f64;
    let (k, n) = (k as f64, n as f64);
    let p = k / n;
    let centre = (p + z * z / (2.0 * n)) / (1.0 + z * z / n);
    let half = z * (p * (1.0 - p) / n + z * z / (4.0 * n * n)).sqrt() / (1.0 + z * z / n);
    ((centre - half).max(0.0), (centre + half).min(1.0))
}

/// The strength a record implies: `None` unless there are at least two franchises, at least
/// one season and between one title and one per season.
pub fn strength_for_titles(franchises: usize, seasons: usize, titles: usize) -> Option<Strength> {
    if franchises < 2 || seasons == 0 || titles == 0 || titles > seasons {
        return None;
    }
    let probability = titles as f64 / seasons as f64;
    let (probability_low, probability_high) = wilson(titles, seasons);
    // The tail rises with the chance, so the coin-flip chance is found by bisection.
    let (mut lo, mut hi) = (0.0_f64, 1.0_f64);
    for _ in 0..80 {
        let mid = (lo + hi) / 2.0;
        if p_franchise_at_least(mid, seasons, titles) < 0.5 {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    let even_odds_probability = (lo + hi) / 2.0;
    Some(Strength {
        franchises,
        seasons,
        titles,
        probability,
        alpha: alpha_for(probability, franchises),
        probability_low,
        probability_high,
        alpha_low: alpha_for(probability_low, franchises),
        alpha_high: alpha_for(probability_high, franchises),
        p_at_least: p_franchise_at_least(probability, seasons, titles),
        p_at_least_equal: p_franchise_at_least(1.0 / franchises as f64, seasons, titles),
        even_odds_probability,
        even_odds_alpha: alpha_for(even_odds_probability, franchises),
    })
}

fn factorials(n: usize) -> Vec<f64> {
    let mut f = vec![1.0f64; n + 1];
    for i in 1..=n {
        f[i] = f[i - 1] * i as f64;
    }
    f
}

fn binomial(n: usize, k: usize) -> f64 {
    if k > n {
        return 0.0;
    }
    let k = k.min(n - k);
    let mut r = 1.0;
    for i in 0..k {
        r = r * (n - i) as f64 / (i + 1) as f64;
    }
    r
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_abs_diff_eq;

    #[test]
    fn thirty_teams_twenty_two_seasons_four_titles() {
        let probs = vec![1.0; 30];
        let p = p_any_at_least(&probs, 22, 4);
        assert_abs_diff_eq!(p, 0.1605, epsilon = 0.0006);
    }

    #[test]
    fn trivial_cases() {
        let probs = vec![1.0; 30];
        assert_abs_diff_eq!(p_any_at_least(&probs, 22, 1), 1.0, epsilon = 1e-12);
        assert_abs_diff_eq!(p_any_at_least(&probs, 22, 23), 0.0, epsilon = 1e-12);
        // One dominant franchise
        let probs = vec![1.0, 0.0, 0.0];
        assert_abs_diff_eq!(p_any_at_least(&probs, 5, 5), 1.0, epsilon = 1e-12);
    }

    #[test]
    fn two_equal_franchises_matches_binomial() {
        // P(max >= 3 of 4 seasons) with two fair franchises = P(X>=3)+P(X<=1) = 2*5/16
        let probs = vec![1.0, 1.0];
        assert_abs_diff_eq!(p_any_at_least(&probs, 4, 3), 10.0 / 16.0, epsilon = 1e-12);
        assert_abs_diff_eq!(p_franchise_at_least(0.5, 4, 3), 5.0 / 16.0, epsilon = 1e-12);
    }

    #[test]
    fn strength_reads_a_record_backwards() {
        // Four titles in 22 seasons among 30: an 18.2 percent title chance, alpha 29 * 4 / 18.
        let s = strength_for_titles(30, 22, 4).unwrap();
        assert_abs_diff_eq!(s.probability, 4.0 / 22.0, epsilon = 1e-12);
        assert_abs_diff_eq!(s.alpha.unwrap(), 29.0 * 4.0 / 18.0, epsilon = 1e-9);
        assert_abs_diff_eq!(s.p_at_least, 0.586, epsilon = 0.001);
        assert_abs_diff_eq!(s.p_at_least_equal, 0.0056, epsilon = 0.0002);
        assert_abs_diff_eq!(s.even_odds_alpha.unwrap(), 5.71, epsilon = 0.01);
        assert_abs_diff_eq!(
            p_franchise_at_least(s.even_odds_probability, 22, 4),
            0.5,
            epsilon = 1e-9
        );
        assert_abs_diff_eq!(s.probability_low, 0.073, epsilon = 0.001);
        assert_abs_diff_eq!(s.probability_high, 0.385, epsilon = 0.001);
        assert_abs_diff_eq!(s.alpha_low.unwrap(), 2.29, epsilon = 0.01);
        assert_abs_diff_eq!(s.alpha_high.unwrap(), 18.17, epsilon = 0.01);
        // Over the eight seasons of the run itself the same four titles mean alpha 29.
        let run = strength_for_titles(30, 8, 4).unwrap();
        assert_abs_diff_eq!(run.alpha.unwrap(), 29.0, epsilon = 1e-9);
        assert!(run.alpha_low.unwrap() < 29.0 && run.alpha_high.unwrap() > 29.0);
        // A title every season has no finite alpha; the interval still has one at its low end.
        let sure = strength_for_titles(30, 5, 5).unwrap();
        assert_eq!(sure.alpha, None);
        assert_abs_diff_eq!(sure.p_at_least, 1.0, epsilon = 1e-12);
        assert!(sure.alpha_low.is_some());
        // Bad inputs.
        assert!(strength_for_titles(1, 22, 4).is_none());
        assert!(strength_for_titles(30, 0, 0).is_none());
        assert!(strength_for_titles(30, 22, 0).is_none());
        assert!(strength_for_titles(30, 22, 23).is_none());
    }

    #[test]
    fn max_distribution_sums_to_one() {
        let probs = vec![1.0; 30];
        let d = max_titles_distribution(&probs, 22);
        assert_abs_diff_eq!(d.iter().sum::<f64>(), 1.0, epsilon = 1e-9);
        assert_abs_diff_eq!(d[4..].iter().sum::<f64>(), 0.1605, epsilon = 0.0006);
    }
}
