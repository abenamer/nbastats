//! The full-strength rating: a team's Elo with the absences of the games it played taken
//! out of the expectation, so the rating measures the full team.
//!
//! The stored Elo blames the whole team for a loss played without its best players, and a
//! forecast that then takes those players' worth off tonight counts the same absence twice.
//! The replay goes through the season's played games again from each team's first rating,
//! with the same FiveThirtyEight update, except that each game's expectation sets each side
//! at its rating less the Elo it was missing that night. A team that lost without its star
//! loses less, and a team that won short-handed gains more. With no absence the replay gives
//! the stored ratings back.
//!
//! Who a side was missing is read from the stored injury reports, confirmed by the box
//! score: a player the report of the game day (or the day before) listed with any status,
//! who then has no line with minutes in the game, was missing. He counts at his worth.

use std::collections::{HashMap, HashSet};

use chrono::NaiveDate;
use serde::Serialize;

use crate::domain::bpm::BoxLine;
use crate::domain::elo;
use crate::domain::ids::{PlayerId, TeamId};
use crate::domain::models::Game;
use crate::domain::spell::line_played;
use crate::domain::status::ReportedStatus;
use crate::domain::worth;

/// The Elo each side was missing, by `(game_id, team)`.
pub type MissingElo = HashMap<(i64, TeamId), f64>;

/// Named in the answers that use it.
pub const FULL_STRENGTH_MODEL: &str = "full-strength-1.0";
/// A report counts for a game dated this many days after it at most: the report of the
/// game day, else the day before (reports are read in the morning).
pub const REPORT_MAX_AGE_DAYS: i64 = 1;

/// The Elo a side is missing with `players` out: each at the expected penalty of his worth
/// `(elo, se)` (`worth::expected_penalty`). A worth at or below zero counts nothing, as it
/// prices nothing tonight; so does a player without a worth, or one whose penalty is not
/// finite (a worth with an error that is not), rather than the cap. The sum is capped at
/// [`worth::MAX_ABSENCE_ELO`], so a night of resting starters cannot run a rating update
/// past where its margin formula holds.
pub fn missing_elo<'a>(
    players: impl IntoIterator<Item = &'a PlayerId>,
    worth_of: &HashMap<PlayerId, (f64, f64)>,
) -> f64 {
    players
        .into_iter()
        .filter_map(|p| worth_of.get(p))
        .filter(|(elo, _)| *elo > 0.0)
        .map(|(elo, se)| worth::expected_penalty(*elo, *se))
        .filter(|p| p.is_finite())
        .fold(0.0, |acc, p| acc + p)
        .min(worth::MAX_ABSENCE_ELO)
}

/// The Elo each side was missing in each played game, by `(game_id, team)`: the players
/// the latest stored report within [`REPORT_MAX_AGE_DAYS`] of the game listed for the team,
/// who then did not play, priced by [`missing_elo`]. The report date is chosen across all
/// teams, so a team with nobody on that report had nobody missing. A game the team has no
/// box-score line for counts nothing: who played is not known.
pub fn game_absences(
    games: &[Game],
    lines: &[BoxLine],
    reports: &[ReportedStatus],
    worth_of: &HashMap<PlayerId, (f64, f64)>,
) -> MissingElo {
    // Who played each game for each team, and which teams have lines for it.
    let mut played: HashSet<(i64, PlayerId)> = HashSet::new();
    let mut with_lines: HashSet<(i64, TeamId)> = HashSet::new();
    for l in lines {
        with_lines.insert((l.game_id, l.team_id));
        if line_played(l) {
            played.insert((l.game_id, l.player_id));
        }
    }
    // The stored report dates, and each team's report lines by date.
    let mut dates: Vec<NaiveDate> = reports.iter().map(|r| r.report_date).collect();
    dates.sort_unstable();
    dates.dedup();
    let mut by_team: HashMap<(TeamId, NaiveDate), Vec<&ReportedStatus>> = HashMap::new();
    for r in reports {
        by_team
            .entry((r.team_id, r.report_date))
            .or_default()
            .push(r);
    }
    let mut out = HashMap::new();
    for g in games.iter().filter(|g| g.is_played()) {
        for team in [g.home_team_id, g.away_team_id] {
            if !with_lines.contains(&(g.id, team)) {
                continue;
            }
            // The latest stored report dated the game day or within the age limit before.
            let oldest = g.game_date - chrono::Duration::days(REPORT_MAX_AGE_DAYS);
            let Some(report_date) = dates
                .iter()
                .rev()
                .find(|d| **d <= g.game_date && **d >= oldest)
            else {
                continue;
            };
            let Some(team_lines) = by_team.get(&(team, *report_date)) else {
                continue;
            };
            let missing = missing_elo(
                team_lines
                    .iter()
                    .filter(|r| !played.contains(&(g.id, r.player_id)))
                    .map(|r| &r.player_id),
                worth_of,
            );
            if missing > 0.0 {
                out.insert((g.id, team), missing);
            }
        }
    }
    out
}

/// A team's rating moved from the stored Elo to its full-strength rating, for a forecast
/// that prices the injury report: the report's absences then come off the full team.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, serde::Deserialize)]
pub struct RatingShift {
    pub team_id: TeamId,
    pub elo: f64,
}

/// The largest shift a forecast accepts, either way.
pub const MAX_SHIFT_ELO: f64 = 400.0;
/// The most shifts a forecast accepts: more than any league's teams.
pub const MAX_SHIFTS: usize = 64;

/// One team's full-strength rating, with what went into it.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct FullStrength {
    pub team_id: TeamId,
    /// The rating with the absences out of the expectation.
    pub rating: f64,
    /// The stored rating at the same point, for comparison.
    pub stored: f64,
    /// Games replayed, and those the team played short (with Elo missing).
    pub games: u32,
    pub games_short: u32,
    /// The Elo missing over those games, summed.
    pub elo_missing: f64,
}

/// Replay the played games of one season dated `until` or before, in order, from each
/// team's first pre-game rating, with `missing[(game_id, team)]` Elo taken off each side's
/// rating in the game's expectation and the FiveThirtyEight update applied to the full
/// ratings. A team with no game played keeps its first rating. Every team of `games`
/// gets a row.
pub fn replay(
    games: &[Game],
    missing: &MissingElo,
    until: NaiveDate,
) -> HashMap<TeamId, FullStrength> {
    let mut ordered: Vec<&Game> = games.iter().collect();
    ordered.sort_by_key(|g| (g.game_date, g.seq, g.id));
    let mut out: HashMap<TeamId, FullStrength> = HashMap::new();
    for g in &ordered {
        for (team, pre) in [
            (g.home_team_id, g.home_elo_pre),
            (g.away_team_id, g.away_elo_pre),
        ] {
            out.entry(team).or_insert(FullStrength {
                team_id: team,
                rating: pre,
                stored: pre,
                games: 0,
                games_short: 0,
                elo_missing: 0.0,
            });
        }
    }
    for g in ordered
        .iter()
        .filter(|g| g.game_date <= until && g.is_played())
    {
        let home_out = missing.get(&(g.id, g.home_team_id)).copied().unwrap_or(0.0);
        let away_out = missing.get(&(g.id, g.away_team_id)).copied().unwrap_or(0.0);
        let home_full = out[&g.home_team_id].rating;
        let away_full = out[&g.away_team_id].rating;
        let Some(delta) = step(g, home_full, away_full, home_out, away_out) else {
            continue;
        };
        for (team, change, short, stored) in [
            (g.home_team_id, delta, home_out, g.home_elo_post),
            (g.away_team_id, -delta, away_out, g.away_elo_post),
        ] {
            if let Some(row) = out.get_mut(&team) {
                row.rating += change;
                row.stored = stored;
                row.games += 1;
                if short > 0.0 {
                    row.games_short += 1;
                    row.elo_missing += short;
                }
            }
        }
    }
    out
}

/// One game's update at the ratings the sides played at (each side's full rating less the
/// Elo it was missing): the home side's change, which the away side takes with the opposite
/// sign, and which goes to the full ratings. None for a tie, which is no result (the stored
/// chain does not rate it either), or an update that is not finite.
fn step(g: &Game, home_full: f64, away_full: f64, home_out: f64, away_out: f64) -> Option<f64> {
    let (h, a) = (g.home_score?, g.away_score?);
    if h == a {
        return None;
    }
    let (home_post, _) =
        elo::fivethirtyeight_update(home_full - home_out, away_full - away_out, h, a, g.neutral);
    let delta = home_post - (home_full - home_out);
    delta.is_finite().then_some(delta)
}

/// Each played game's full-strength pre-game ratings, `(home, away)` by game id: the
/// replay of [`replay`] read before each game's update instead of at a date. A team's
/// first played game reads its stored pre-game rating, so with no absence every game reads
/// its stored ratings back.
pub fn pregame_ratings(games: &[Game], missing: &MissingElo) -> HashMap<i64, (f64, f64)> {
    let mut ordered: Vec<&Game> = games.iter().filter(|g| g.is_played()).collect();
    ordered.sort_by_key(|g| (g.game_date, g.seq, g.id));
    let mut rating: HashMap<TeamId, f64> = HashMap::new();
    let mut out = HashMap::with_capacity(ordered.len());
    for g in ordered {
        let home_full = *rating.entry(g.home_team_id).or_insert(g.home_elo_pre);
        let away_full = *rating.entry(g.away_team_id).or_insert(g.away_elo_pre);
        out.insert(g.id, (home_full, away_full));
        let home_out = missing.get(&(g.id, g.home_team_id)).copied().unwrap_or(0.0);
        let away_out = missing.get(&(g.id, g.away_team_id)).copied().unwrap_or(0.0);
        if let Some(delta) = step(g, home_full, away_full, home_out, away_out) {
            rating.insert(g.home_team_id, home_full + delta);
            rating.insert(g.away_team_id, away_full - delta);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ids::SeasonYear;
    use crate::domain::models::{EloSource, GameType, ResultSource};

    fn d(day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 11, day).unwrap()
    }

    fn game(id: i64, day: u32, home: i32, away: i32, score: Option<(i32, i32)>) -> Game {
        Game {
            id,
            season: SeasonYear::of(2027),
            game_date: d(day),
            seq: 0,
            home_team_id: TeamId::new(home),
            away_team_id: TeamId::new(away),
            neutral: false,
            game_type: GameType::Regular,
            home_score: score.map(|s| s.0),
            away_score: score.map(|s| s.1),
            home_elo_pre: 1500.0,
            away_elo_pre: 1500.0,
            home_elo_post: 1500.0,
            away_elo_post: 1500.0,
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

    /// A chain of games with the stored ratings computed as the application computes them.
    fn season() -> Vec<Game> {
        let mut games = vec![
            game(1, 1, 1, 2, Some((100, 110))),
            game(2, 3, 2, 1, Some((95, 105))),
            game(3, 5, 1, 2, Some((120, 90))),
            game(4, 9, 2, 1, None),
        ];
        let mut rating: HashMap<TeamId, f64> =
            [(TeamId::new(1), 1600.0), (TeamId::new(2), 1450.0)].into();
        for g in games.iter_mut() {
            g.home_elo_pre = rating[&g.home_team_id];
            g.away_elo_pre = rating[&g.away_team_id];
            if let (Some(h), Some(a)) = (g.home_score, g.away_score) {
                let (hp, ap) =
                    elo::fivethirtyeight_update(g.home_elo_pre, g.away_elo_pre, h, a, g.neutral);
                g.home_elo_post = hp;
                g.away_elo_post = ap;
                rating.insert(g.home_team_id, hp);
                rating.insert(g.away_team_id, ap);
            } else {
                g.home_elo_post = g.home_elo_pre;
                g.away_elo_post = g.away_elo_pre;
            }
        }
        games
    }

    #[test]
    fn without_absences_the_replay_gives_the_stored_ratings_back() {
        let games = season();
        let r = replay(&games, &HashMap::new(), d(30));
        for (team, post) in [(1, games[2].home_elo_post), (2, games[2].away_elo_post)] {
            let row = r[&TeamId::new(team)];
            assert!((row.rating - post).abs() < 1e-9 && (row.stored - post).abs() < 1e-9);
            assert_eq!((row.games, row.games_short), (3, 0));
        }
        // Read before the first game, a team keeps its first rating.
        let early = replay(&games, &HashMap::new(), d(1) - chrono::Duration::days(1));
        assert_eq!(early[&TeamId::new(1)].rating, 1600.0);
    }

    #[test]
    fn the_pregame_ratings_are_the_replay_read_before_each_game() {
        let games = season();
        // Without absences, every played game reads its stored pre-game ratings back; the
        // unplayed game has none.
        let plain = pregame_ratings(&games, &HashMap::new());
        assert_eq!(plain.len(), 3);
        for g in &games[..3] {
            let (h, a) = plain[&g.id];
            assert!((h - g.home_elo_pre).abs() < 1e-9 && (a - g.away_elo_pre).abs() < 1e-9);
        }
        // Team 1 lost game 1 at home short-handed: before game 2 its full rating is above
        // the stored one, by what the replay read at the date between the games says, and
        // before game 1 nothing has changed yet.
        let missing: HashMap<(i64, TeamId), f64> = [((1, TeamId::new(1)), 120.0)].into();
        let full = pregame_ratings(&games, &missing);
        assert_eq!(full[&1], plain[&1]);
        let between = replay(&games, &missing, d(2));
        // Game 2: team 2 is home, team 1 away.
        let (h2, a2) = full[&2];
        assert!((h2 - between[&TeamId::new(2)].rating).abs() < 1e-9);
        assert!((a2 - between[&TeamId::new(1)].rating).abs() < 1e-9);
        assert!(a2 > games[1].away_elo_pre && h2 < games[1].home_elo_pre);
        // The pricing helper: a worth at or below zero and a penalty that is not finite
        // count nothing, and the side is capped.
        let worth: HashMap<PlayerId, (f64, f64)> = [
            (PlayerId::new(1), (90.0, 0.0)),
            (PlayerId::new(2), (-30.0, 20.0)),
            (PlayerId::new(3), (50.0, f64::INFINITY)),
            (PlayerId::new(4), (500.0, 0.0)),
        ]
        .into();
        let ids = [PlayerId::new(1), PlayerId::new(2), PlayerId::new(3)];
        assert_eq!(missing_elo(&ids, &worth), 90.0);
        assert_eq!(
            missing_elo(&[PlayerId::new(1), PlayerId::new(4)], &worth),
            worth::MAX_ABSENCE_ELO
        );
        assert_eq!(missing_elo(&[PlayerId::new(9)], &worth), 0.0);
    }

    #[test]
    fn a_loss_played_short_costs_the_full_team_less() {
        let games = season();
        // Team 1 lost game 1 at home without 120 Elo of players.
        let missing: HashMap<(i64, TeamId), f64> = [((1, TeamId::new(1)), 120.0)].into();
        let full = replay(&games, &missing, d(1));
        let plain = replay(&games, &HashMap::new(), d(1));
        let t1 = full[&TeamId::new(1)];
        assert!(t1.rating > plain[&TeamId::new(1)].rating);
        assert!(full[&TeamId::new(2)].rating < plain[&TeamId::new(2)].rating);
        assert_eq!((t1.games, t1.games_short, t1.elo_missing), (1, 1, 120.0));
        // The update is zero-sum: what one gains the other loses.
        let total = |r: &HashMap<TeamId, FullStrength>| r.values().map(|x| x.rating).sum::<f64>();
        assert!((total(&full) - total(&plain)).abs() < 1e-9);
    }

    fn line(game_id: i64, player: i32, team: i32, seconds: i32) -> BoxLine {
        BoxLine {
            game_id,
            player_id: PlayerId::new(player),
            team_id: TeamId::new(team),
            seconds: Some(seconds),
            pts: None,
            reb: None,
            oreb: None,
            dreb: None,
            ast: None,
            stl: None,
            blk: None,
            tov: None,
            pf: None,
            fgm: None,
            fga: None,
            fg3m: None,
            ftm: None,
            fta: None,
        }
    }

    fn listed(day: u32, player: i32, team: i32) -> ReportedStatus {
        ReportedStatus {
            report_date: d(day),
            player_id: PlayerId::new(player),
            team_id: TeamId::new(team),
            status: worth::AbsenceStatus::Out,
        }
    }

    #[test]
    fn a_listed_player_who_did_not_play_counts_at_his_worth() {
        let games = season();
        let lines = vec![
            // Game 1: player 10 (listed) did not play, player 11 (listed) played.
            line(1, 11, 1, 1800),
            line(1, 12, 1, 2000),
            line(1, 20, 2, 2000),
            // Game 3: team 1 has lines, the report is three days old.
            line(3, 12, 1, 2000),
        ];
        let reports = vec![
            listed(1, 10, 1),
            listed(1, 11, 1),
            listed(1, 20, 2),
            listed(2, 10, 1),
        ];
        let worth: HashMap<PlayerId, (f64, f64)> = [
            (PlayerId::new(10), (90.0, 0.0)),
            (PlayerId::new(11), (50.0, 0.0)),
            (PlayerId::new(20), (30.0, 0.0)),
        ]
        .into();
        let out = game_absences(&games, &lines, &reports, &worth);
        // Player 10 missed game 1; 11 and 20 played it.
        assert_eq!(out.get(&(1, TeamId::new(1))), Some(&90.0));
        assert_eq!(out.get(&(1, TeamId::new(2))), None);
        // Game 2 (day 3): the report of day 2 counts, but team 1 has no lines for it.
        assert_eq!(out.get(&(2, TeamId::new(1))), None);
        // Game 3 (day 5): the latest report is three days old.
        assert_eq!(out.get(&(3, TeamId::new(1))), None);
    }

    #[test]
    fn the_report_date_is_one_for_all_teams_and_odd_cases_count_nothing() {
        let games = season();
        let lines = vec![
            // Game 1 (day 1): player 10 dressed for 0 seconds; 11 played.
            line(1, 10, 1, 0),
            line(1, 11, 1, 1800),
            line(1, 20, 2, 2000),
            // Game 2 (day 3): both teams have lines; nobody listed played.
            line(2, 12, 1, 2000),
            line(2, 21, 2, 2000),
        ];
        let reports = vec![
            listed(1, 10, 1),
            listed(1, 13, 1),
            // Day 2 lists team 1's player 10; day 3, the game day, lists only team 2.
            listed(2, 10, 1),
            listed(3, 22, 2),
            listed(3, 23, 2),
        ];
        let worth: HashMap<PlayerId, (f64, f64)> = [
            (PlayerId::new(10), (90.0, 0.0)),
            // A worth below zero prices nothing, as tonight.
            (PlayerId::new(13), (-30.0, 20.0)),
            (PlayerId::new(22), (500.0, 0.0)),
            (PlayerId::new(23), (50.0, f64::INFINITY)),
        ]
        .into();
        let out = game_absences(&games, &lines, &reports, &worth);
        // Zero seconds is not playing; the negative worth adds nothing.
        assert_eq!(out.get(&(1, TeamId::new(1))), Some(&90.0));
        // On day 3 the report of day 3 stands for every team: team 1 is not on it, so
        // its day-2 line does not count; team 2's player is capped at 400.
        assert_eq!(out.get(&(2, TeamId::new(1))), None);
        assert_eq!(out.get(&(2, TeamId::new(2))), Some(&worth::MAX_ABSENCE_ELO));
    }

    #[test]
    fn a_short_side_that_wins_stays_finite_and_ties_neutral_floors_and_the_cutoff_hold() {
        let mut games = season();
        // A tie (no result) and a neutral game on day 7.
        games[1].home_score = Some(100);
        games[1].away_score = Some(100);
        games[1].home_elo_post = games[1].home_elo_pre;
        games[1].away_elo_post = games[1].away_elo_pre;
        games[2].neutral = true;
        // Team 2 wins game 1 away at the cap of missing Elo.
        let missing: HashMap<(i64, TeamId), f64> =
            [((1, TeamId::new(2)), worth::MAX_ABSENCE_ELO)].into();
        let r = replay(&games, &missing, d(30));
        assert!(r.values().all(|x| x.rating.is_finite()));
        let plain = replay(&games, &HashMap::new(), d(30));
        assert!(r[&TeamId::new(2)].rating > plain[&TeamId::new(2)].rating);
        // The tie is not rated: two games each, not three.
        assert_eq!(plain[&TeamId::new(1)].games, 2);
        // The neutral game, rated without the bonus, matches the update's own reading.
        let g = &games[2];
        let (h, a) = (g.home_score.unwrap(), g.away_score.unwrap());
        let before = replay(&games, &HashMap::new(), d(4));
        let (hp, _) = elo::fivethirtyeight_update(
            before[&TeamId::new(1)].rating,
            before[&TeamId::new(2)].rating,
            h,
            a,
            true,
        );
        assert!((plain[&TeamId::new(1)].rating - hp).abs() < 1e-9);
        // A game on the cutoff date counts; the day before, it does not.
        assert_eq!(
            replay(&games, &HashMap::new(), d(5))[&TeamId::new(1)].games,
            2
        );
        assert_eq!(
            replay(&games, &HashMap::new(), d(4))[&TeamId::new(1)].games,
            1
        );
    }
}
