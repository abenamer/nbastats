//! Elo continuation: the application's own ratings for every game.
//!
//! Starting from each team's rating at the end of the previous season, apply
//! FiveThirtyEight's between-season reversion and then its game update, game by game in
//! date order. Games without a result (a schedule) carry the current rating forward
//! unchanged. A franchise that has no previous season starts at 1300, as in the archive.

use std::collections::HashMap;

use chrono::NaiveDate;

use crate::domain::elo;
use crate::domain::ids::{FranchiseId, TeamId};

/// The rating of a franchise in its first season (the archive's rule for expansion teams and
/// for the ABA's first season).
pub const NEW_FRANCHISE_ELO: f64 = 1300.0;

/// Start ratings of a season. Each team takes the reverted end rating of its franchise from
/// the previous season (so a relocated or renamed team continues its lineage). A franchise
/// without one takes `seed` when the caller has a reference rating for the team (an archive
/// season imported without its predecessor), else [`NEW_FRANCHISE_ELO`].
pub fn season_start(
    teams: &[(TeamId, FranchiseId)],
    previous_end: &HashMap<FranchiseId, f64>,
    seed: &HashMap<TeamId, f64>,
) -> HashMap<TeamId, f64> {
    teams
        .iter()
        .map(|&(team, franchise)| {
            let start = match previous_end.get(&franchise) {
                Some(&end) => elo::season_reversion(end),
                None => seed.get(&team).copied().unwrap_or(NEW_FRANCHISE_ELO),
            };
            (team, start)
        })
        .collect()
}

/// A game result (or scheduled game) to rate.
#[derive(Debug, Clone, PartialEq)]
pub struct ResultGame {
    pub date: NaiveDate,
    pub home: TeamId,
    pub away: TeamId,
    pub neutral: bool,
    pub playoff: bool,
    pub home_score: Option<i32>,
    pub away_score: Option<i32>,
}

/// A rated game.
#[derive(Debug, Clone, PartialEq)]
pub struct RatedGame {
    pub game: ResultGame,
    pub home_elo_pre: f64,
    pub away_elo_pre: f64,
    pub home_elo_post: f64,
    pub away_elo_post: f64,
    pub home_win_prob: f64,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ContinuationError {
    #[error("no previous rating for {0}; the previous season has no games for this team")]
    MissingRating(TeamId),
}

/// Starting ratings for a new season: the previous season's final ratings after reversion.
pub fn season_start_ratings(previous_end: &HashMap<TeamId, f64>) -> HashMap<TeamId, f64> {
    previous_end
        .iter()
        .map(|(&t, &e)| (t, elo::season_reversion(e)))
        .collect()
}

/// Rate `games` (any order; they are sorted by date) from `start` ratings. Every team in the
/// games must have a start rating.
pub fn rate_games(
    start: &HashMap<TeamId, f64>,
    games: &[ResultGame],
) -> Result<Vec<RatedGame>, ContinuationError> {
    let mut ratings = start.clone();
    let mut ordered: Vec<&ResultGame> = games.iter().collect();
    ordered.sort_by_key(|g| (g.date, g.home, g.away));
    let mut out = Vec::with_capacity(games.len());
    for g in ordered {
        let home_pre = *ratings
            .get(&g.home)
            .ok_or(ContinuationError::MissingRating(g.home))?;
        let away_pre = *ratings
            .get(&g.away)
            .ok_or(ContinuationError::MissingRating(g.away))?;
        let home_win_prob = elo::game_probability(
            home_pre,
            away_pre,
            true,
            g.neutral,
            elo::DEFAULT_HOME_ADVANTAGE,
            1.0,
        );
        let (home_post, away_post) = match (g.home_score, g.away_score) {
            (Some(h), Some(a)) if h != a => {
                elo::fivethirtyeight_update(home_pre, away_pre, h, a, g.neutral)
            }
            _ => (home_pre, away_pre),
        };
        ratings.insert(g.home, home_post);
        ratings.insert(g.away, away_post);
        out.push(RatedGame {
            game: g.clone(),
            home_elo_pre: home_pre,
            away_elo_pre: away_pre,
            home_elo_post: home_post,
            away_elo_post: away_post,
            home_win_prob,
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_abs_diff_eq;

    #[test]
    fn season_start_follows_the_franchise_then_the_seed_then_1300() {
        let (t1, t2, t3) = (TeamId::new(1), TeamId::new(2), TeamId::new(3));
        let (f1, f2, f3) = (
            FranchiseId::new(10),
            FranchiseId::new(20),
            FranchiseId::new(30),
        );
        // t1 relocated: a new team id, the same franchise as last season's end rating.
        let previous_end = HashMap::from([(f1, 1705.0)]);
        let seed = HashMap::from([(t2, 1432.5)]);
        let start = season_start(&[(t1, f1), (t2, f2), (t3, f3)], &previous_end, &seed);
        assert_abs_diff_eq!(start[&t1], 0.75 * 1705.0 + 0.25 * 1505.0, epsilon = 1e-9);
        assert_abs_diff_eq!(start[&t2], 1432.5, epsilon = 1e-9);
        assert_abs_diff_eq!(start[&t3], NEW_FRANCHISE_ELO, epsilon = 1e-9);
    }

    fn d(day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2025, 10, day).unwrap()
    }

    #[test]
    fn ratings_chain_through_games_and_schedule() {
        let (a, b) = (TeamId::new(1), TeamId::new(2));
        let start: HashMap<TeamId, f64> = [(a, 1600.0), (b, 1500.0)].into_iter().collect();
        let games = vec![
            ResultGame {
                date: d(22),
                home: b,
                away: a,
                neutral: false,
                playoff: false,
                home_score: None,
                away_score: None,
            },
            ResultGame {
                date: d(21),
                home: a,
                away: b,
                neutral: false,
                playoff: false,
                home_score: Some(110),
                away_score: Some(100),
            },
        ];
        let rated = rate_games(&start, &games).unwrap();
        assert_eq!(rated[0].game.date, d(21));
        assert!(rated[0].home_elo_post > 1600.0);
        assert_abs_diff_eq!(
            rated[0].home_elo_post + rated[0].away_elo_post,
            3100.0,
            epsilon = 1e-9
        );
        // the scheduled game starts from the updated ratings and leaves them unchanged
        assert_abs_diff_eq!(
            rated[1].away_elo_pre,
            rated[0].home_elo_post,
            epsilon = 1e-12
        );
        assert_eq!(rated[1].home_elo_post, rated[1].home_elo_pre);
    }

    #[test]
    fn missing_rating_is_an_error() {
        let start: HashMap<TeamId, f64> = [(TeamId::new(1), 1500.0)].into_iter().collect();
        let g = ResultGame {
            date: d(1),
            home: TeamId::new(1),
            away: TeamId::new(9),
            neutral: false,
            playoff: false,
            home_score: Some(1),
            away_score: Some(0),
        };
        assert_eq!(
            rate_games(&start, &[g]),
            Err(ContinuationError::MissingRating(TeamId::new(9)))
        );
    }

    #[test]
    fn reversion_applies_to_every_team() {
        let prev: HashMap<TeamId, f64> = [(TeamId::new(1), 1705.0), (TeamId::new(2), 1305.0)]
            .into_iter()
            .collect();
        let start = season_start_ratings(&prev);
        assert_abs_diff_eq!(start[&TeamId::new(1)], 1655.0, epsilon = 1e-9);
        assert_abs_diff_eq!(start[&TeamId::new(2)], 1355.0, epsilon = 1e-9);
    }
}
