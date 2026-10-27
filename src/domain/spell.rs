//! Absence spells: how long a regular player stays out once he is out, read from the game
//! logs, and the runs of missed games in progress today.
//!
//! The injury report says who is out tonight; its return date is most often the next game,
//! whatever the injury. The game logs hold thousands of finished runs of missed games, and
//! the length of a run that has already lasted `k` games is a well-measured thing. The
//! survival curve of the runs (Kaplan-Meier, so a run cut off by the end of the season
//! still counts for the games it did last) gives the expected number of games still to be
//! missed by a player who has missed `k` so far. The forecast prices an absence with it when
//! the report gives no better date.

use std::collections::{BTreeMap, HashMap};

use chrono::NaiveDate;
use serde::Serialize;

use crate::domain::bpm::BoxLine;
use crate::domain::ids::{PlayerId, SeasonYear, TeamId};
use crate::domain::models::{Game, GameType};
use crate::domain::recovery::{MIN_PRE_MINUTES, MIN_PRE_PLAYED, WINDOW};

/// Named in the availability answer, so a reader knows which rule priced the spans.
pub const SPELL_MODEL_VERSION: &str = "spell-survival-1.0";
/// Runs below this count give no curve: the status defaults stand.
pub const MIN_RUNS: usize = 50;
/// The expected remaining games are reported for runs already this long, from zero.
pub const CURVE_LENGTH: u32 = 20;

/// One run of team games a regular player did not play.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Run {
    /// Team games missed in a row.
    pub missed: u32,
    /// The run reached the end of the team's games with lines: its true length is at
    /// least `missed`.
    pub censored: bool,
}

/// A run in progress at the team's latest game with lines.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct CurrentRun {
    pub team_id: TeamId,
    pub player_id: PlayerId,
    pub missed: u32,
    /// He was a regular player over the twenty team games before the run (fifteen played
    /// at fifteen minutes a game on average).
    pub regular: bool,
}

/// A team's latest regular-season game with lines: where its runs are read up to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct LinesThrough {
    pub team_id: TeamId,
    pub game_date: NaiveDate,
}

/// Whether a line says the player played: minutes above zero, or counting statistics when
/// the minutes are unknown (some stored seasons carry lines without them).
pub fn line_played(l: &BoxLine) -> bool {
    match l.seconds {
        Some(s) => s > 0,
        None => {
            [l.pts, l.fga, l.fta, l.reb, l.ast]
                .iter()
                .map(|v| v.unwrap_or(0))
                .sum::<i32>()
                > 0
        }
    }
}

/// The runs of one season.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SeasonRuns {
    pub season: SeasonYear,
    /// Teams with at least one game with lines.
    pub teams_with_lines: u32,
    /// Each team's latest game with lines.
    pub lines_through: Vec<LinesThrough>,
    /// Finished and censored runs of regular players, each with twenty team games before.
    pub runs: Vec<Run>,
    /// Every player in a run at his team's latest game with lines.
    pub current: Vec<CurrentRun>,
}

/// One player's games with one team, in schedule order.
struct Sequence {
    played: Vec<bool>,
    minutes: Vec<f64>,
}

impl Sequence {
    /// A regular player over the twenty team games before index `s`.
    fn regular_before(&self, s: usize) -> bool {
        if s < WINDOW {
            return false;
        }
        let window = s - WINDOW..s;
        let played = window.clone().filter(|i| self.played[*i]).count() as u32;
        if played < MIN_PRE_PLAYED {
            return false;
        }
        let minutes: f64 = window
            .filter(|i| self.played[*i])
            .map(|i| self.minutes[i])
            .sum();
        minutes / f64::from(played) >= MIN_PRE_MINUTES
    }
}

/// The runs of a season from its regular-season games with box-score lines. A run must have
/// twenty team games before it with the player a regular; a run that reaches the end of the
/// team's games with lines is censored, unless the player then appears with another team
/// (a trade, not an injury), in which case it is dropped. `current` holds every player's
/// run at the team's latest game with lines, regular or not, so a report line can be given
/// the games already missed.
pub fn season_runs(season: SeasonYear, games: &[Game], lines: &[BoxLine]) -> SeasonRuns {
    let mut ordered: Vec<&Game> = games
        .iter()
        .filter(|g| g.game_type == GameType::Regular && g.is_played())
        .collect();
    ordered.sort_by_key(|g| (g.game_date, g.seq, g.id));
    let order: HashMap<i64, usize> = ordered.iter().enumerate().map(|(i, g)| (g.id, i)).collect();
    // Each team's lines per game index, and each player's last game played with any team.
    let mut by_team: BTreeMap<TeamId, BTreeMap<usize, Vec<&BoxLine>>> = BTreeMap::new();
    let mut last_played: HashMap<PlayerId, (usize, TeamId)> = HashMap::new();
    for l in lines {
        let Some(&i) = order.get(&l.game_id) else {
            continue;
        };
        by_team
            .entry(l.team_id)
            .or_default()
            .entry(i)
            .or_default()
            .push(l);
        if line_played(l) {
            let e = last_played.entry(l.player_id).or_insert((i, l.team_id));
            if i >= e.0 {
                *e = (i, l.team_id);
            }
        }
    }
    let mut runs = Vec::new();
    let mut current = Vec::new();
    let mut lines_through = Vec::new();
    for (team, per_game) in &by_team {
        let indices: Vec<usize> = per_game.keys().copied().collect();
        let n = indices.len();
        if n == 0 {
            continue;
        }
        lines_through.push(LinesThrough {
            team_id: *team,
            game_date: ordered[indices[n - 1]].game_date,
        });
        let mut ids: Vec<PlayerId> = per_game
            .values()
            .flat_map(|ls| ls.iter())
            .filter(|l| line_played(l))
            .map(|l| l.player_id)
            .collect();
        ids.sort_unstable();
        ids.dedup();
        for player in ids {
            let mut seq = Sequence {
                played: Vec::with_capacity(n),
                minutes: Vec::with_capacity(n),
            };
            for i in &indices {
                let line = per_game[i].iter().find(|l| l.player_id == player);
                seq.played.push(line.is_some_and(|l| line_played(l)));
                seq.minutes
                    .push(f64::from(line.and_then(|l| l.seconds).unwrap_or(0)) / 60.0);
            }
            // Traded away: his last game played was with another team.
            let elsewhere = last_played.get(&player).is_some_and(|(_, t)| t != team);
            let mut i = 0;
            while i < n {
                if seq.played[i] {
                    i += 1;
                    continue;
                }
                let start = i;
                while i < n && !seq.played[i] {
                    i += 1;
                }
                let end = i;
                let missed = (end - start) as u32;
                let regular = seq.regular_before(start);
                if end == n {
                    if elsewhere {
                        break;
                    }
                    current.push(CurrentRun {
                        team_id: *team,
                        player_id: player,
                        missed,
                        regular,
                    });
                    if regular {
                        runs.push(Run {
                            missed,
                            censored: true,
                        });
                    }
                    break;
                }
                if regular {
                    runs.push(Run {
                        missed,
                        censored: false,
                    });
                }
            }
        }
    }
    SeasonRuns {
        season,
        teams_with_lines: by_team.len() as u32,
        lines_through,
        runs,
        current,
    }
}

/// The survival curve of the runs: the share still out after `t` games missed.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Survival {
    pub model: &'static str,
    pub runs: u32,
    pub censored: u32,
    /// The longest run seen, finished or not: the curve is read as ending there.
    pub longest: u32,
    /// `survival[t]` is the share of runs longer than `t` games, for `t` from zero.
    pub survival: Vec<f64>,
}

impl Survival {
    /// Kaplan-Meier over the runs: at each length, the runs that end there over the runs
    /// at least that long; a censored run counts as at risk up to its length and never as
    /// an end. None below [`MIN_RUNS`] runs.
    pub fn fit(runs: &[Run]) -> Option<Survival> {
        if runs.len() < MIN_RUNS {
            return None;
        }
        let longest = runs.iter().map(|r| r.missed).max().unwrap_or(0);
        let mut survival = Vec::with_capacity(longest as usize + 1);
        let mut s = 1.0;
        survival.push(s);
        for t in 1..=longest {
            let at_risk = runs.iter().filter(|r| r.missed >= t).count() as f64;
            let ended = runs.iter().filter(|r| !r.censored && r.missed == t).count() as f64;
            if at_risk > 0.0 {
                s *= 1.0 - ended / at_risk;
            }
            survival.push(s);
        }
        Some(Survival {
            model: SPELL_MODEL_VERSION,
            runs: runs.len() as u32,
            censored: runs.iter().filter(|r| r.censored).count() as u32,
            longest,
            survival,
        })
    }

    /// The share of runs longer than `t` games; zero past the longest run.
    pub fn beyond(&self, t: u32) -> f64 {
        if t >= self.longest {
            0.0
        } else {
            self.survival[t as usize]
        }
    }

    /// The games a player will still miss, tonight's included, given that he has missed
    /// `missed` and is out tonight: the mean of the run's remaining length over the runs
    /// longer than `missed`. At least one game.
    pub fn expected_remaining(&self, missed: u32) -> f64 {
        let base = self.beyond(missed);
        if base <= 0.0 {
            return 1.0;
        }
        let tail: f64 = (missed..self.longest).map(|t| self.beyond(t)).sum();
        (tail / base).max(1.0)
    }

    /// The mean run length.
    pub fn mean_run(&self) -> f64 {
        self.expected_remaining(0)
    }

    /// `expected_remaining` for runs already zero to [`CURVE_LENGTH`] games long.
    pub fn curve(&self) -> Vec<f64> {
        (0..=CURVE_LENGTH)
            .map(|k| self.expected_remaining(k))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::models::{EloSource, ResultSource};

    const S: SeasonYear = SeasonYear::of(2016);

    fn run(missed: u32, censored: bool) -> Run {
        Run { missed, censored }
    }

    /// Enough copies of a small set of runs to pass the minimum.
    fn many(base: &[Run]) -> Vec<Run> {
        base.iter()
            .copied()
            .cycle()
            .take(MIN_RUNS.max(base.len()) / base.len() * base.len() + base.len())
            .collect()
    }

    #[test]
    fn the_curve_is_the_share_of_runs_still_going_and_the_mean_follows() {
        // Lengths 1, 1, 2, 3, 5, all finished: the mean is 2.4.
        let runs = many(&[
            run(1, false),
            run(1, false),
            run(2, false),
            run(3, false),
            run(5, false),
        ]);
        let s = Survival::fit(&runs).unwrap();
        assert_eq!(s.longest, 5);
        assert!((s.beyond(0) - 1.0).abs() < 1e-12);
        assert!((s.beyond(1) - 0.6).abs() < 1e-12);
        assert!((s.beyond(2) - 0.4).abs() < 1e-12);
        assert!((s.beyond(3) - 0.2).abs() < 1e-12);
        assert!((s.beyond(4) - 0.2).abs() < 1e-12);
        assert_eq!(s.beyond(5), 0.0);
        assert!((s.mean_run() - 2.4).abs() < 1e-12);
        // Two games missed and still out: the runs of 3 and 5 remain, 1 and 3 more games.
        assert!((s.expected_remaining(2) - 2.0).abs() < 1e-12);
        // Past the longest run the answer is one game, never zero.
        assert_eq!(s.expected_remaining(9), 1.0);
        assert_eq!(s.curve().len(), CURVE_LENGTH as usize + 1);
        assert_eq!(s.censored, 0);
    }

    #[test]
    fn a_censored_run_counts_as_at_risk_and_never_as_an_end() {
        // A run of 2 and a run cut off at 3: half the runs end at 2, none are known to end
        // at 3, and the curve is read as ending at the longest length.
        let runs = many(&[run(2, false), run(3, true)]);
        let s = Survival::fit(&runs).unwrap();
        assert!((s.beyond(1) - 1.0).abs() < 1e-12);
        assert!((s.beyond(2) - 0.5).abs() < 1e-12);
        assert_eq!(s.beyond(3), 0.0);
        assert_eq!(s.censored as usize, runs.len() / 2);
        // Mean: 1 + 1 + 0.5 = 2.5 games.
        assert!((s.mean_run() - 2.5).abs() < 1e-12);
        // Too few runs: no curve.
        assert!(Survival::fit(&runs[..MIN_RUNS - 1]).is_none());
    }

    fn game(id: i64, day: i64, home: i32, away: i32) -> Game {
        Game {
            id,
            season: S,
            game_date: chrono::NaiveDate::from_ymd_opt(2015, 11, 1).unwrap()
                + chrono::Duration::days(day),
            seq: 0,
            home_team_id: TeamId::new(home),
            away_team_id: TeamId::new(away),
            neutral: false,
            game_type: GameType::Regular,
            home_score: Some(100),
            away_score: Some(90),
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
        }
    }

    fn line(game_id: i64, player: i32, team: i32, minutes: i32) -> BoxLine {
        BoxLine {
            game_id,
            player_id: PlayerId::new(player),
            team_id: TeamId::new(team),
            seconds: Some(minutes * 60),
            pts: Some(10),
            reb: Some(4),
            oreb: Some(1),
            dreb: Some(3),
            ast: Some(2),
            stl: Some(1),
            blk: Some(0),
            tov: Some(1),
            pf: Some(2),
            fgm: Some(4),
            fga: Some(9),
            fg3m: Some(1),
            ftm: Some(1),
            fta: Some(2),
        }
    }

    /// Two teams, sixty games against each other. Team 1: player 11 misses games 25..31
    /// and comes back; player 12 is out from game 50 to the end (censored); player 13 is
    /// traded to team 2 after game 40 (no run); player 14 plays eight minutes a game, so
    /// his run at 30..34 is nobody's. Team 2: player 21 misses the last three games.
    fn fixture() -> (Vec<Game>, Vec<BoxLine>) {
        let mut games = Vec::new();
        let mut lines = Vec::new();
        for k in 0..60usize {
            let id = k as i64 + 1;
            games.push(game(id, id, 1, 2));
            let mut push = |player: i32, team: i32, minutes: i32| {
                if minutes > 0 {
                    lines.push(line(id, player, team, minutes));
                }
            };
            push(11, 1, if (25..31).contains(&k) { 0 } else { 30 });
            push(12, 1, if k >= 50 { 0 } else { 30 });
            if k <= 40 {
                push(13, 1, 30);
            } else {
                push(13, 2, 30);
            }
            push(14, 1, if (30..34).contains(&k) { 0 } else { 8 });
            push(15, 1, 30);
            push(21, 2, if k >= 57 { 0 } else { 30 });
            push(22, 2, 30);
        }
        (games, lines)
    }

    #[test]
    fn runs_are_found_with_their_censoring_and_the_trade_is_dropped() {
        let (games, lines) = fixture();
        let r = season_runs(S, &games, &lines);
        assert_eq!(r.teams_with_lines, 2);
        let mut found = r.runs.clone();
        found.sort_by_key(|x| (x.missed, x.censored));
        // Player 21's three games at the end (censored), player 11's six, player 12's ten
        // to the end (censored). Player 13's run with team 1 after the trade is dropped,
        // and player 14 is not a regular.
        assert_eq!(found, vec![run(3, true), run(6, false), run(10, true)]);
        assert_eq!(r.lines_through.len(), 2);
        assert!(r
            .lines_through
            .iter()
            .all(|t| t.game_date == games.last().unwrap().game_date));
        // A line without minutes counts by its statistics.
        let mut no_minutes = line(1, 99, 1, 0);
        no_minutes.seconds = None;
        assert!(line_played(&no_minutes));
        no_minutes.pts = Some(0);
        no_minutes.fga = Some(0);
        no_minutes.fta = Some(0);
        no_minutes.reb = Some(0);
        no_minutes.ast = Some(0);
        assert!(!line_played(&no_minutes));
        let mut current: Vec<(i32, i32, u32, bool)> = r
            .current
            .iter()
            .map(|c| {
                (
                    c.team_id.as_i32(),
                    c.player_id.as_i32(),
                    c.missed,
                    c.regular,
                )
            })
            .collect();
        current.sort_unstable();
        assert_eq!(current, vec![(1, 12, 10, true), (2, 21, 3, true)]);
    }

    #[test]
    fn a_season_without_lines_has_nothing() {
        let (games, _) = fixture();
        let r = season_runs(S, &games, &[]);
        assert_eq!(r.teams_with_lines, 0);
        assert!(r.runs.is_empty() && r.current.is_empty());
    }
}
