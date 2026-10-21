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
    (0..=seasons).map(|j| (cdf[j + 1] - cdf[j]).max(0.0)).collect()
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
    fn max_distribution_sums_to_one() {
        let probs = vec![1.0; 30];
        let d = max_titles_distribution(&probs, 22);
        assert_abs_diff_eq!(d.iter().sum::<f64>(), 1.0, epsilon = 1e-9);
        assert_abs_diff_eq!(d[4..].iter().sum::<f64>(), 0.1605, epsilon = 0.0006);
    }
}
