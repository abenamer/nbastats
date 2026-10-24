//! The home-court bonus a season's forecasts use.
//!
//! The rating updates keep FiveThirtyEight's 100 points, so the ratings stay on the archive's
//! scale. For forecasts of regular-season games the bonus is fitted on the regular-season
//! games of the seasons before the one forecast (the previous [`elo::HOME_ADVANTAGE_WINDOW`]
//! seasons, at least [`elo::HOME_ADVANTAGE_MIN_GAMES`] games): the home advantage has fallen
//! from about 85 Elo points in the 2000s to about 40 in the 2020s, and the fitted value
//! lowers the out-of-sample log loss of the game forecasts. Playoff games keep the default:
//! the postseason fit moves too much from one year to the next to help.

use crate::domain::elo::{self, HomeAdvantage};
use crate::domain::ids::SeasonYear;
use crate::domain::models::{Game, GameType};

/// The fit samples a season's games give: played, non-neutral regular-season games as
/// (home rating minus away rating before the game, home won).
pub fn samples(games: &[Game]) -> Vec<(f64, bool)> {
    games
        .iter()
        .filter(|g| g.game_type == GameType::Regular && !g.neutral)
        .filter(|g| g.home_elo_pre.is_finite() && g.away_elo_pre.is_finite())
        .filter_map(|g| {
            g.home_won()
                .map(|won| (g.home_elo_pre - g.away_elo_pre, won))
        })
        .collect()
}

/// The seasons whose games fit the bonus of `season`: the window before it, oldest first.
pub fn window(season: SeasonYear) -> Vec<SeasonYear> {
    (1..=elo::HOME_ADVANTAGE_WINDOW as i32)
        .rev()
        .filter_map(|back| SeasonYear::try_new(season.end_year() - back).ok())
        .collect()
}

/// The bonus for `season` from the samples of each season of its window, oldest first,
/// with the seasons that had games.
pub fn from_samples(per_season: &[(SeasonYear, Vec<(f64, bool)>)]) -> HomeAdvantage {
    let all: Vec<(f64, bool)> = per_season
        .iter()
        .flat_map(|(_, s)| s.iter().copied())
        .collect();
    let used: Vec<SeasonYear> = per_season
        .iter()
        .filter(|(_, s)| !s.is_empty())
        .map(|(season, _)| *season)
        .collect();
    match elo::fit_home_advantage(&all) {
        Some(regular) => HomeAdvantage {
            regular,
            playoff: elo::DEFAULT_HOME_ADVANTAGE,
            fitted_from: used.first().copied().zip(used.last().copied()),
            games: all.len(),
        },
        None => HomeAdvantage {
            games: all.len(),
            ..HomeAdvantage::DEFAULT
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_window_is_the_five_seasons_before() {
        assert_eq!(
            window(SeasonYear::of(2025))
                .iter()
                .map(|s| s.end_year())
                .collect::<Vec<_>>(),
            [2020, 2021, 2022, 2023, 2024]
        );
        // The first seasons of the archive have a shorter window.
        assert_eq!(window(SeasonYear::of(1948)).len(), 1);
        assert!(window(SeasonYear::of(1947)).is_empty());
    }

    fn game(game_type: GameType, neutral: bool, score: Option<(i32, i32)>, gap: f64) -> Game {
        Game {
            id: 0,
            season: SeasonYear::of(2020),
            game_date: chrono::NaiveDate::from_ymd_opt(2019, 11, 1).unwrap(),
            seq: 0,
            home_team_id: crate::domain::ids::TeamId::new(1),
            away_team_id: crate::domain::ids::TeamId::new(2),
            neutral,
            game_type,
            home_score: score.map(|s| s.0),
            away_score: score.map(|s| s.1),
            home_elo_pre: 1500.0 + gap,
            away_elo_pre: 1500.0,
            home_elo_post: 1500.0,
            away_elo_post: 1500.0,
            home_win_prob: None,
            elo_source: crate::domain::models::EloSource::Computed,
            result_source: crate::domain::models::ResultSource::Archive,
            ref_home_elo_pre: None,
            ref_away_elo_pre: None,
            ref_home_elo_post: None,
            ref_away_elo_post: None,
        }
    }

    #[test]
    fn only_played_regular_season_games_on_a_home_court_are_samples() {
        let games = vec![
            game(GameType::Regular, false, Some((100, 90)), 30.0),
            game(GameType::Regular, false, Some((90, 100)), -20.0),
            game(GameType::Regular, true, Some((100, 90)), 0.0),
            game(GameType::Playoff, false, Some((100, 90)), 0.0),
            game(GameType::Regular, false, None, 0.0),
        ];
        assert_eq!(samples(&games), vec![(30.0, true), (-20.0, false)]);
    }

    #[test]
    fn enough_games_fit_the_bonus_and_name_the_seasons_used() {
        // Games drawn with a 40-point home edge, spread over three seasons with an empty one
        // between them: the fit recovers the edge and names the first and last season used.
        let mut seed = 7u64;
        let mut draw = || {
            (0..1200)
                .map(|_| {
                    seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
                    let u = (seed >> 11) as f64 / (1u64 << 53) as f64;
                    seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
                    let gap = ((seed >> 11) as f64 / (1u64 << 53) as f64 - 0.5) * 400.0;
                    let p = elo::game_probability(1500.0 + gap, 1500.0, true, false, 40.0, 1.0);
                    (gap, u < p)
                })
                .collect::<Vec<_>>()
        };
        let per: Vec<(SeasonYear, Vec<(f64, bool)>)> = [2016, 2017, 2018, 2019]
            .into_iter()
            .map(|y| {
                let s = if y == 2018 { Vec::new() } else { draw() };
                (SeasonYear::of(y), s)
            })
            .collect();
        let h = from_samples(&per);
        assert_eq!(h.games, 3600);
        assert_eq!(
            h.fitted_from,
            Some((SeasonYear::of(2016), SeasonYear::of(2019)))
        );
        assert!((h.regular - 40.0).abs() < 25.0, "fitted {}", h.regular);
        assert_eq!(h.playoff, elo::DEFAULT_HOME_ADVANTAGE);
    }

    #[test]
    fn too_few_games_keep_the_default() {
        let per = vec![(SeasonYear::of(2020), vec![(0.0, true); 10])];
        let h = from_samples(&per);
        assert_eq!(h.regular, elo::DEFAULT_HOME_ADVANTAGE);
        assert_eq!(h.fitted_from, None);
        assert_eq!(h.games, 10);
        let h = from_samples(&[]);
        assert_eq!(h, HomeAdvantage::DEFAULT);
    }
}
