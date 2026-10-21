//! Concentration and probability helpers shared by the engines.

/// Gini coefficient of a non-negative vector (0 = perfect equality). Returns 0 for an empty
/// vector or an all-zero vector.
pub fn gini(values: &[f64]) -> f64 {
    let n = values.len();
    if n == 0 {
        return 0.0;
    }
    let mut v: Vec<f64> = values.to_vec();
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let total: f64 = v.iter().sum();
    if total <= 0.0 {
        return 0.0;
    }
    let mut weighted = 0.0;
    for (i, x) in v.iter().enumerate() {
        weighted += (i as f64 + 1.0) * x;
    }
    (2.0 * weighted) / (n as f64 * total) - (n as f64 + 1.0) / n as f64
}

/// Herfindahl–Hirschman index of shares (sum of squared shares). Values are normalised first.
/// Returns 0 if everything is zero.
pub fn hhi(values: &[f64]) -> f64 {
    let total: f64 = values.iter().sum();
    if total <= 0.0 {
        return 0.0;
    }
    values.iter().map(|v| (v / total).powi(2)).sum()
}

/// Share of the total held by the `k` largest entries.
pub fn top_k_share(values: &[f64], k: usize) -> f64 {
    let total: f64 = values.iter().sum();
    if total <= 0.0 {
        return 0.0;
    }
    let mut v: Vec<f64> = values.to_vec();
    v.sort_by(|a, b| b.partial_cmp(a).unwrap());
    v.iter().take(k).sum::<f64>() / total
}

/// Standard error of a Monte Carlo probability estimate.
pub fn binomial_se(p: f64, n: u64) -> f64 {
    if n == 0 {
        return 0.0;
    }
    (p * (1.0 - p) / n as f64).sqrt()
}

/// Exact distribution of the number of successes in independent Bernoulli trials with
/// probabilities `probs` (Poisson-binomial). Returns pmf[0..=n].
pub fn poisson_binomial_pmf(probs: &[f64]) -> Vec<f64> {
    let mut pmf = vec![1.0f64];
    for &p in probs {
        let mut next = vec![0.0f64; pmf.len() + 1];
        for (k, &mass) in pmf.iter().enumerate() {
            next[k] += mass * (1.0 - p);
            next[k + 1] += mass * p;
        }
        pmf = next;
    }
    pmf
}

/// P(X >= k) for a pmf.
pub fn tail_at_least(pmf: &[f64], k: usize) -> f64 {
    pmf.iter().skip(k).sum()
}

/// Wilson score interval (95%) for a proportion.
pub fn wilson_interval(successes: f64, n: f64) -> (f64, f64) {
    if n <= 0.0 {
        return (0.0, 0.0);
    }
    let z = 1.959964;
    let p = successes / n;
    let denom = 1.0 + z * z / n;
    let centre = (p + z * z / (2.0 * n)) / denom;
    let half = z * ((p * (1.0 - p) / n + z * z / (4.0 * n * n)).sqrt()) / denom;
    ((centre - half).max(0.0), (centre + half).min(1.0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_abs_diff_eq;

    #[test]
    fn gini_equal_is_zero() {
        assert_abs_diff_eq!(gini(&[1.0, 1.0, 1.0, 1.0]), 0.0, epsilon = 1e-12);
    }

    #[test]
    fn gini_one_takes_all() {
        // For n=4, max Gini = (n-1)/n = 0.75
        assert_abs_diff_eq!(gini(&[0.0, 0.0, 0.0, 4.0]), 0.75, epsilon = 1e-12);
    }

    #[test]
    fn hhi_values() {
        assert_abs_diff_eq!(hhi(&[1.0, 1.0, 1.0, 1.0]), 0.25, epsilon = 1e-12);
        assert_abs_diff_eq!(hhi(&[0.0, 5.0]), 1.0, epsilon = 1e-12);
    }

    #[test]
    fn top_share() {
        assert_abs_diff_eq!(top_k_share(&[1.0, 2.0, 3.0, 4.0], 2), 0.7, epsilon = 1e-12);
    }

    #[test]
    fn poisson_binomial_matches_binomial() {
        let pmf = poisson_binomial_pmf(&[0.5; 4]);
        assert_abs_diff_eq!(pmf[2], 6.0 / 16.0, epsilon = 1e-12);
        assert_abs_diff_eq!(pmf.iter().sum::<f64>(), 1.0, epsilon = 1e-12);
        assert_abs_diff_eq!(tail_at_least(&pmf, 3), 5.0 / 16.0, epsilon = 1e-12);
    }
}
