//! Elo rating mathematics: game win probabilities, home-court adjustments, series
//! probabilities and the simple rating update used for optional in-simulation updating.

use serde::Serialize;

use crate::domain::ids::SeasonYear;

/// Default home-court advantage in Elo points (FiveThirtyEight's NBA model uses 100). The
/// rating updates always use it, so the ratings stay on the archive's scale; forecasts of
/// regular-season games use the fitted value of [`HomeAdvantage`] instead.
pub const DEFAULT_HOME_ADVANTAGE: f64 = 100.0;
/// Seasons of regular-season games the home advantage of a season is fitted on: the ones
/// before it.
pub const HOME_ADVANTAGE_WINDOW: usize = 5;
/// Fewest games for a fit; with fewer the default stands.
pub const HOME_ADVANTAGE_MIN_GAMES: usize = 2000;

/// The home-court bonus a season's forecasts use: for the regular season the value fitted on
/// the seasons before it, for the playoffs the default, whose fit moves too much from one
/// postseason to the next to help.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct HomeAdvantage {
    pub regular: f64,
    pub playoff: f64,
    /// The seasons the regular value was fitted on, first and last; `None` when the default
    /// stands.
    pub fitted_from: Option<(SeasonYear, SeasonYear)>,
    /// Regular-season games the fit used.
    pub games: usize,
}

impl HomeAdvantage {
    pub const DEFAULT: HomeAdvantage = HomeAdvantage {
        regular: DEFAULT_HOME_ADVANTAGE,
        playoff: DEFAULT_HOME_ADVANTAGE,
        fitted_from: None,
        games: 0,
    };
}

/// Mean log loss of the Elo forecast over `samples` of (home rating minus away rating,
/// home won) with home bonus `bonus`.
fn home_log_loss(samples: &[(f64, bool)], bonus: f64) -> f64 {
    let total: f64 = samples
        .iter()
        .map(|&(diff, home_won)| {
            let p = 1.0 / (1.0 + 10f64.powf(-(diff + bonus) / 400.0));
            -(if home_won { p } else { 1.0 - p }).ln()
        })
        .sum();
    total / samples.len() as f64
}

/// The lowest and highest bonus the fit may return; a fit that lands on either is rejected.
pub const HOME_ADVANTAGE_FIT_RANGE: (f64, f64) = (-100.0, 300.0);

/// The home bonus that minimises the log loss of the Elo forecast over `samples`, on the Elo
/// scale within [`HOME_ADVANTAGE_FIT_RANGE`]; `None` with fewer than
/// [`HOME_ADVANTAGE_MIN_GAMES`] samples, with a sample that is not a finite number, or when
/// the minimum sits within a point of either end of the range (degenerate samples, such as
/// a home win in every game, drive the search to an end). The log loss is convex in the
/// bonus, so a ternary search finds the minimum.
pub fn fit_home_advantage(samples: &[(f64, bool)]) -> Option<f64> {
    if samples.len() < HOME_ADVANTAGE_MIN_GAMES || samples.iter().any(|(d, _)| !d.is_finite()) {
        return None;
    }
    let (mut lo, mut hi) = HOME_ADVANTAGE_FIT_RANGE;
    for _ in 0..60 {
        let m1 = lo + (hi - lo) / 3.0;
        let m2 = hi - (hi - lo) / 3.0;
        if home_log_loss(samples, m1) < home_log_loss(samples, m2) {
            hi = m2;
        } else {
            lo = m1;
        }
    }
    let bonus = (lo + hi) / 2.0;
    let (floor, ceiling) = HOME_ADVANTAGE_FIT_RANGE;
    (bonus - floor > 1.0 && ceiling - bonus > 1.0).then_some(bonus)
}
/// FiveThirtyEight's K-factor.
pub const FIVETHIRTYEIGHT_K: f64 = 20.0;
/// Rating every team is pulled toward between seasons.
pub const SEASON_MEAN: f64 = 1505.0;
/// Share of last season's deviation from the mean that carries over.
pub const SEASON_CARRYOVER: f64 = 0.75;

/// FiveThirtyEight's between-season reversion: 75% of the previous rating, 25% of 1505.
/// Verified against the dataset: every team's first pre-game rating of a season equals this
/// function of its last post-game rating of the previous season.
pub fn season_reversion(previous_end: f64) -> f64 {
    SEASON_CARRYOVER * previous_end + (1.0 - SEASON_CARRYOVER) * SEASON_MEAN
}

/// FiveThirtyEight's margin-of-victory multiplier: ((margin + 3)^0.8) / (7.5 + 0.006 × d),
/// where `d` is the winner's rating advantage including home court (negative for an upset).
pub fn margin_multiplier(margin: i32, winner_advantage: f64) -> f64 {
    (f64::from(margin.abs()) + 3.0).powf(0.8) / (7.5 + 0.006 * winner_advantage)
}

/// One game's rating change for the home team under FiveThirtyEight's model. Returns
/// (home_post, away_post). Verified to reproduce the dataset to four decimals.
pub fn fivethirtyeight_update(
    home_elo: f64,
    away_elo: f64,
    home_score: i32,
    away_score: i32,
    neutral: bool,
) -> (f64, f64) {
    let bonus = if neutral { 0.0 } else { DEFAULT_HOME_ADVANTAGE };
    let p_home = win_probability(home_elo + bonus, away_elo);
    let home_won = home_score > away_score;
    let margin = home_score - away_score;
    let winner_advantage = if home_won {
        (home_elo + bonus) - away_elo
    } else {
        away_elo - (home_elo + bonus)
    };
    let delta = FIVETHIRTYEIGHT_K
        * margin_multiplier(margin, winner_advantage)
        * (if home_won { 1.0 } else { 0.0 } - p_home);
    (home_elo + delta, away_elo - delta)
}

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
    fn fivethirtyeight_update_matches_dataset_rows() {
        // 2016 Finals game 7: GSW (home) 1764.828667 vs CLE 1749.839437, 89-93 -> 1755.639 / 1759.029104
        let (h, a) = fivethirtyeight_update(1764.828667, 1749.839437, 89, 93, false);
        assert_abs_diff_eq!(h, 1755.639, epsilon = 1e-3);
        assert_abs_diff_eq!(a, 1759.029104, epsilon = 1e-3);
        // 2025-03-17: UTA (home) 1324.006775 vs CHI 1429.289651, 97-111 -> 1311.393947 / 1441.902479
        let (h, a) = fivethirtyeight_update(1324.006775, 1429.289651, 97, 111, false);
        assert_abs_diff_eq!(h, 1311.393947, epsilon = 1e-3);
        assert_abs_diff_eq!(a, 1441.902479, epsilon = 1e-3);
    }

    #[test]
    fn season_reversion_is_three_quarters() {
        assert_abs_diff_eq!(season_reversion(1705.0), 1655.0, epsilon = 1e-12);
        assert_abs_diff_eq!(season_reversion(1505.0), 1505.0, epsilon = 1e-12);
    }

    #[test]
    fn update_moves_ratings_symmetrically() {
        let (a, b) = update(1500.0, 1500.0, 0.5, true, 20.0);
        assert_abs_diff_eq!(a, 1510.0, epsilon = 1e-12);
        assert_abs_diff_eq!(b, 1490.0, epsilon = 1e-12);
    }

    #[test]
    fn the_home_advantage_fit_recovers_the_bonus_the_games_were_drawn_with() {
        // Deterministic samples: for each rating gap, the share of home wins the Elo model
        // gives at a bonus of 60 points, so the fit must land near 60.
        let mut samples = Vec::new();
        for i in 0..40 {
            let diff = -200.0 + 10.0 * i as f64;
            let p = 1.0 / (1.0 + 10f64.powf(-(diff + 60.0) / 400.0));
            let wins = (p * 100.0).round() as usize;
            for j in 0..100 {
                samples.push((diff, j < wins));
            }
        }
        let fit = fit_home_advantage(&samples).unwrap();
        assert!((fit - 60.0).abs() < 3.0, "{fit}");
        assert!(home_log_loss(&samples, fit) <= home_log_loss(&samples, 100.0));
        // Too few games: no fit.
        assert_eq!(fit_home_advantage(&samples[..100]), None);
        // Degenerate samples drive the search to an end of the range: no fit either.
        let all_home: Vec<_> = samples.iter().map(|(d, _)| (*d, true)).collect();
        assert_eq!(fit_home_advantage(&all_home), None);
        let all_away: Vec<_> = samples.iter().map(|(d, _)| (*d, false)).collect();
        assert_eq!(fit_home_advantage(&all_away), None);
        let mut with_nan = samples.clone();
        with_nan[0].0 = f64::NAN;
        assert_eq!(fit_home_advantage(&with_nan), None);
        assert_eq!(HomeAdvantage::DEFAULT.regular, DEFAULT_HOME_ADVANTAGE);
    }
}
