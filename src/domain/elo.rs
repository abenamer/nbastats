//! Elo rating mathematics: game win probabilities, home-court adjustments, series
//! probabilities and the simple rating update used for optional in-simulation updating.

/// Default home-court advantage in Elo points (FiveThirtyEight's NBA model uses 100).
pub const DEFAULT_HOME_ADVANTAGE: f64 = 100.0;

/// Probability that a team rated `elo_a` beats a team rated `elo_b` on a neutral floor.
///
/// P(A beats B) = 1 / (1 + 10^((E_B - E_A) / 400))
pub fn win_probability(elo_a: f64, elo_b: f64) -> f64 {
    1.0 / (1.0 + 10f64.powf((elo_b - elo_a) / 400.0))
}

/// Win probability for the home team including a home-court bonus (in Elo points).
pub fn home_win_probability(home_elo: f64, away_elo: f64, home_advantage: f64) -> f64 {
    win_probability(home_elo + home_advantage, away_elo)
}

/// Win probability for team A against B where `a_home` says whether A hosts.
/// `neutral` disables the home-court bonus.
pub fn game_probability(
    elo_a: f64,
    elo_b: f64,
    a_home: bool,
    neutral: bool,
    home_advantage: f64,
    scale: f64,
) -> f64 {
    let diff = (elo_a - elo_b) * scale;
    let adj = if neutral {
        0.0
    } else if a_home {
        home_advantage
    } else {
        -home_advantage
    };
    1.0 / (1.0 + 10f64.powf(-(diff + adj) / 400.0))
}

/// Simple Elo update without margin-of-victory multiplier. `k` is typically 20.
/// Returns (new_elo_winner_side_a, new_elo_b) given A's expected score and whether A won.
pub fn update(elo_a: f64, elo_b: f64, expected_a: f64, a_won: bool, k: f64) -> (f64, f64) {
    let actual = if a_won { 1.0 } else { 0.0 };
    let delta = k * (actual - expected_a);
    (elo_a + delta, elo_b - delta)
}

/// Exact probability that team A wins a best-of-`n` series given per-game probabilities.
/// `game_probs[i]` is P(A wins game i+1). Games are played in order until one side reaches
/// the required number of wins. `game_probs.len()` must be at least `n`.
pub fn series_probability(game_probs: &[f64]) -> f64 {
    let n = game_probs.len();
    assert!(
        n % 2 == 1 && n >= 1,
        "series must have an odd number of games"
    );
    let need = n / 2 + 1;
    // dp[a][b] = probability of state with A having a wins and B having b wins
    let mut dp = vec![vec![0.0f64; need + 1]; need + 1];
    dp[0][0] = 1.0;
    let mut p_a = 0.0;
    for (g, &p) in game_probs.iter().enumerate() {
        let mut next = vec![vec![0.0f64; need + 1]; need + 1];
        for a in 0..need {
            for b in 0..need {
                let mass = dp[a][b];
                if mass == 0.0 || a + b != g {
                    continue;
                }
                let win = mass * p;
                let lose = mass * (1.0 - p);
                if a + 1 == need {
                    p_a += win;
                } else {
                    next[a + 1][b] += win;
                }
                if b + 1 < need {
                    next[a][b + 1] += lose;
                }
            }
        }
        dp = next;
    }
    p_a
}

/// Home pattern of a best-of-seven series: `true` means the home-court team hosts.
pub const HOME_PATTERN_2_2_1_1_1: [bool; 7] = [true, true, false, false, true, false, true];
pub const HOME_PATTERN_2_3_2: [bool; 7] = [true, true, false, false, false, true, true];
/// Best-of-five pattern used by the NBA for first-round series 1984–2002 (2-2-1).
pub const HOME_PATTERN_2_2_1: [bool; 5] = [true, true, false, false, true];

/// Per-game probabilities for team A (which holds home court if `a_home_court`) over a series.
pub fn series_game_probs(
    elo_a: f64,
    elo_b: f64,
    a_home_court: bool,
    pattern: &[bool],
    neutral: bool,
    home_advantage: f64,
    scale: f64,
) -> Vec<f64> {
    pattern
        .iter()
        .map(|&hc_team_hosts| {
            let a_home = if a_home_court {
                hc_team_hosts
            } else {
                !hc_team_hosts
            };
            game_probability(elo_a, elo_b, a_home, neutral, home_advantage, scale)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_abs_diff_eq;

    #[test]
    fn equal_ratings_are_a_coin_flip() {
        assert_abs_diff_eq!(win_probability(1500.0, 1500.0), 0.5, epsilon = 1e-12);
    }

    #[test]
    fn four_hundred_points_is_ten_to_one() {
        assert_abs_diff_eq!(
            win_probability(1900.0, 1500.0),
            10.0 / 11.0,
            epsilon = 1e-12
        );
    }

    #[test]
    fn home_court_matches_fivethirtyeight_file() {
        // First row of nba_elo.csv: TRH 1300 vs NYK 1300 at home -> 0.640065
        assert_abs_diff_eq!(
            home_win_probability(1300.0, 1300.0, 100.0),
            0.640065,
            epsilon = 1e-5
        );
        // 2016 Finals G7: GSW 1764.828667 home vs CLE 1749.839437 -> 0.659694076
        assert_abs_diff_eq!(
            home_win_probability(1764.828667, 1749.839437, 100.0),
            0.659694076,
            epsilon = 1e-6
        );
    }

    #[test]
    fn probabilities_are_complementary() {
        let p = game_probability(1600.0, 1500.0, true, false, 100.0, 1.0);
        let q = game_probability(1500.0, 1600.0, false, false, 100.0, 1.0);
        assert_abs_diff_eq!(p + q, 1.0, epsilon = 1e-12);
    }

    #[test]
    fn series_probability_coin_flip() {
        assert_abs_diff_eq!(series_probability(&[0.5; 7]), 0.5, epsilon = 1e-12);
        assert_abs_diff_eq!(series_probability(&[0.5; 5]), 0.5, epsilon = 1e-12);
    }

    #[test]
    fn series_probability_known_value() {
        // P(win best-of-7 | p=0.6 every game) = sum_{k=0}^{3} C(3+k,k) 0.6^4 0.4^k = 0.7102
        assert_abs_diff_eq!(series_probability(&[0.6; 7]), 0.710208, epsilon = 1e-6);
        assert_abs_diff_eq!(series_probability(&[1.0; 7]), 1.0, epsilon = 1e-12);
        assert_abs_diff_eq!(series_probability(&[0.0; 7]), 0.0, epsilon = 1e-12);
    }

    #[test]
    fn series_probability_with_home_pattern_is_order_sensitive_but_symmetric() {
        let a = series_game_probs(
            1600.0,
            1550.0,
            true,
            &HOME_PATTERN_2_2_1_1_1,
            false,
            100.0,
            1.0,
        );
        let b = series_game_probs(
            1550.0,
            1600.0,
            false,
            &HOME_PATTERN_2_2_1_1_1,
            false,
            100.0,
            1.0,
        );
        assert_abs_diff_eq!(
            series_probability(&a) + series_probability(&b),
            1.0,
            epsilon = 1e-12
        );
        assert!(series_probability(&a) > 0.5);
    }

    #[test]
    fn update_moves_ratings_symmetrically() {
        let (a, b) = update(1500.0, 1500.0, 0.5, true, 20.0);
        assert_abs_diff_eq!(a, 1510.0, epsilon = 1e-12);
        assert_abs_diff_eq!(b, 1490.0, epsilon = 1e-12);
    }
}
