//! The strength of a team's schedule: the mean rating of its opponents before each game,
//! over the regular-season games it has played, and the same mean over the games it won
//! and over the games it lost. The Elo Explorer shows the three beside the record.

use std::collections::HashMap;

use serde::Serialize;

use super::ids::TeamId;
use super::models::{Game, GameType};

/// The mean rating of a team's opponents before each regular-season game, over every game
/// played, over the wins and over the losses. A mean is `None` when there is no such game.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Default)]
pub struct OpponentRatings {
    pub games: u32,
    pub faced: Option<f64>,
    pub wins: u32,
    pub beaten: Option<f64>,
    pub losses: u32,
    pub lost_to: Option<f64>,
}

#[derive(Default)]
struct Sums {
    faced: (u32, f64),
    beaten: (u32, f64),
    lost_to: (u32, f64),
}

fn mean((n, sum): (u32, f64)) -> Option<f64> {
    (n > 0).then(|| sum / f64::from(n))
}

/// The opponent ratings of every team with a played regular-season game among `games`.
/// Each game counts the opponent's rating before it, the rating the opponent held when
/// the two met, from either side. Play-in and playoff games are left out, as the record is.
pub fn by_team(games: &[Game]) -> HashMap<TeamId, OpponentRatings> {
    let mut sums: HashMap<TeamId, Sums> = HashMap::new();
    for g in games.iter().filter(|g| g.game_type == GameType::Regular) {
        let Some(home_won) = g.home_won() else {
            continue;
        };
        for (team, opponent_elo, won) in [
            (g.home_team_id, g.away_elo_pre, home_won),
            (g.away_team_id, g.home_elo_pre, !home_won),
        ] {
            let s = sums.entry(team).or_default();
            s.faced.0 += 1;
            s.faced.1 += opponent_elo;
            let side = if won { &mut s.beaten } else { &mut s.lost_to };
            side.0 += 1;
            side.1 += opponent_elo;
        }
    }
    sums.into_iter()
        .map(|(team, s)| {
            (
                team,
                OpponentRatings {
                    games: s.faced.0,
                    faced: mean(s.faced),
                    wins: s.beaten.0,
                    beaten: mean(s.beaten),
                    losses: s.lost_to.0,
                    lost_to: mean(s.lost_to),
                },
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use chrono::NaiveDate;

    use super::*;
    use crate::domain::ids::SeasonYear;
    use crate::domain::models::{EloSource, ResultSource};

    fn game(
        id: i64,
        home: i32,
        away: i32,
        home_elo: f64,
        away_elo: f64,
        score: Option<(i32, i32)>,
        game_type: GameType,
    ) -> Game {
        Game {
            id,
            season: SeasonYear::of(2025),
            game_date: NaiveDate::from_ymd_opt(2024, 11, 1).unwrap() + chrono::Duration::days(id),
            seq: 0,
            home_team_id: TeamId::new(home),
            away_team_id: TeamId::new(away),
            neutral: false,
            game_type,
            home_score: score.map(|s| s.0),
            away_score: score.map(|s| s.1),
            home_elo_pre: home_elo,
            away_elo_pre: away_elo,
            home_elo_post: home_elo,
            away_elo_post: away_elo,
            home_win_prob: None,
            elo_source: EloSource::Computed,
            result_source: ResultSource::Results,
            ref_home_elo_pre: None,
            ref_away_elo_pre: None,
            ref_home_elo_post: None,
            ref_away_elo_post: None,
            cup_stage: None,
        }
    }

    #[test]
    fn the_means_take_the_opponents_rating_before_each_game_from_either_side() {
        let games = vec![
            // Team 1 beats team 2 (rated 1600) at home, loses to team 3 (1400) away.
            game(1, 1, 2, 1500.0, 1600.0, Some((100, 90)), GameType::Regular),
            game(2, 3, 1, 1400.0, 1510.0, Some((100, 90)), GameType::Regular),
            // Team 1 beats team 2 again, now rated 1580: the rating as it stood that day.
            game(3, 1, 2, 1505.0, 1580.0, Some((100, 90)), GameType::Regular),
        ];
        let r = by_team(&games);
        let t1 = r[&TeamId::new(1)];
        assert_eq!((t1.games, t1.wins, t1.losses), (3, 2, 1));
        assert_eq!(t1.faced, Some((1600.0 + 1400.0 + 1580.0) / 3.0));
        assert_eq!(t1.beaten, Some((1600.0 + 1580.0) / 2.0));
        assert_eq!(t1.lost_to, Some(1400.0));
        // Team 3 won its only game, so it has lost to nobody.
        let t3 = r[&TeamId::new(3)];
        assert_eq!((t3.games, t3.wins, t3.losses), (1, 1, 0));
        assert_eq!(t3.beaten, Some(1510.0));
        assert_eq!(t3.lost_to, None);
        // Team 2 lost twice, from the visiting side.
        let t2 = r[&TeamId::new(2)];
        assert_eq!(t2.lost_to, Some(1502.5));
        assert_eq!(t2.beaten, None);
    }

    #[test]
    fn unplayed_play_in_and_playoff_games_do_not_count() {
        let games = vec![
            game(1, 1, 2, 1500.0, 1600.0, Some((100, 90)), GameType::Regular),
            game(2, 1, 2, 1500.0, 1700.0, None, GameType::Regular),
            game(3, 1, 2, 1500.0, 1800.0, Some((100, 90)), GameType::PlayIn),
            game(4, 1, 2, 1500.0, 1900.0, Some((100, 90)), GameType::Playoff),
            game(5, 1, 4, 1500.0, 1300.0, Some((90, 100)), GameType::CupFinal),
        ];
        let r = by_team(&games);
        let t1 = r[&TeamId::new(1)];
        assert_eq!((t1.games, t1.wins, t1.losses), (1, 1, 0));
        assert_eq!(t1.faced, Some(1600.0));
        assert!(!r.contains_key(&TeamId::new(4)));
    }
}
