//! What a report status has meant: for each line the injury report carried on a day, read
//! from the game logs whether the player played his team's next game and how many of the
//! games after it he missed. Grouped by status, that gives the chance he plays tonight
//! (the share of his worth the status takes off) and the games he goes on to miss (the
//! span), measured instead of assumed.
//!
//! A line is read twice over: once as "not yet out" (he played the team's last game before
//! the report) and once as "in a run" (he had already missed it), because a day-to-day
//! line on the first morning and the same line ten games later mean different things.

use std::collections::{BTreeMap, HashMap};

use chrono::NaiveDate;
use serde::Serialize;

use crate::domain::bpm::BoxLine;
use crate::domain::ids::{PlayerId, TeamId};
use crate::domain::models::{Game, GameType};
use crate::domain::spell::line_played;
use crate::domain::worth::AbsenceStatus;

/// Named in the availability answer.
pub const STATUS_MODEL_VERSION: &str = "status-curve-1.0";
/// Games ahead of the report day that a line is followed for.
pub const HORIZON: usize = 30;
/// Lines below this count give no curve for a status: the defaults stand.
pub const MIN_SAMPLES: usize = 50;

/// One line of a stored injury report, reduced to what the measure needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReportedStatus {
    pub report_date: NaiveDate,
    pub player_id: PlayerId,
    pub team_id: TeamId,
    pub status: AbsenceStatus,
}

/// One report line followed through the team's next games.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StatusSample {
    pub status: AbsenceStatus,
    /// He had missed the team's last game with lines before the report day.
    pub in_run: bool,
    /// For each of the team's next games with lines, up to [`HORIZON`]: did he miss it.
    pub missed: Vec<bool>,
}

/// The lines of a season followed through the game logs. A line whose team has no later
/// game with lines yet gives no sample, nor does a status that takes nothing off or out
/// for the season, which is priced by definition. A player with no game played for the
/// team before the report day is not yet out, whatever the team's last game says: that is
/// how the pricing reads him too.
pub fn samples(games: &[Game], lines: &[BoxLine], reports: &[ReportedStatus]) -> Vec<StatusSample> {
    let mut ordered: Vec<&Game> = games
        .iter()
        .filter(|g| g.game_type == GameType::Regular && g.is_played())
        .collect();
    ordered.sort_by_key(|g| (g.game_date, g.seq, g.id));
    let order: HashMap<i64, usize> = ordered.iter().enumerate().map(|(i, g)| (g.id, i)).collect();
    // Each team's games with lines, in order, and who played each.
    let mut by_team: BTreeMap<TeamId, BTreeMap<usize, Vec<PlayerId>>> = BTreeMap::new();
    for l in lines {
        let Some(&i) = order.get(&l.game_id) else {
            continue;
        };
        let entry = by_team.entry(l.team_id).or_default().entry(i).or_default();
        if line_played(l) {
            entry.push(l.player_id);
        }
    }
    let mut out = Vec::new();
    for r in reports {
        if r.status.weight() <= 0.0 || r.status == AbsenceStatus::OutForSeason {
            continue;
        }
        let Some(team_games) = by_team.get(&r.team_id) else {
            continue;
        };
        let played = |i: &usize| team_games[i].contains(&r.player_id);
        let mut before = team_games
            .keys()
            .filter(|i| ordered[**i].game_date < r.report_date);
        let played_before = before.clone().any(&played);
        let in_run = played_before && before.next_back().is_some_and(|i| !played(i));
        let missed: Vec<bool> = team_games
            .keys()
            .filter(|i| ordered[**i].game_date >= r.report_date)
            .take(HORIZON)
            .map(|i| !played(i))
            .collect();
        if missed.is_empty() {
            continue;
        }
        out.push(StatusSample {
            status: r.status,
            in_run,
            missed,
        });
    }
    out
}

/// What a status has meant, for lines not yet in a run or in one.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct StatusCurve {
    pub status: AbsenceStatus,
    pub in_run: bool,
    pub samples: u32,
    /// `miss[j]` is the share of the lines whose player missed the team's `j`-th next game
    /// (from zero), over the lines that had that many games to follow.
    pub miss: Vec<f64>,
    /// How many lines each `miss[j]` rests on.
    pub observed: Vec<u32>,
}

impl StatusCurve {
    /// The chance the player misses tonight's game: the share of his worth the status
    /// takes off.
    pub fn weight(&self) -> f64 {
        self.miss.first().copied().unwrap_or(0.0)
    }

    /// The games he misses over the horizon, in expectation.
    pub fn expected_missed(&self) -> f64 {
        self.miss.iter().sum()
    }

    /// The span, in games, that at the weight gives the expected games missed; never
    /// beyond the [`HORIZON`] followed, which a small weight would otherwise exceed.
    pub fn span_games(&self) -> f64 {
        let w = self.weight();
        if w > 0.0 {
            (self.expected_missed() / w).min(HORIZON as f64)
        } else {
            0.0
        }
    }
}

/// The curves of every status with enough lines.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct StatusTable {
    pub model: &'static str,
    /// Lines followed, over every status and both states.
    pub lines: u32,
    pub curves: Vec<StatusCurve>,
}

impl StatusTable {
    pub fn fit(samples: &[StatusSample]) -> StatusTable {
        let mut groups: BTreeMap<(u8, bool), Vec<&StatusSample>> = BTreeMap::new();
        for s in samples {
            groups
                .entry((status_key(s.status), s.in_run))
                .or_default()
                .push(s);
        }
        let curves = groups
            .into_iter()
            .filter(|(_, g)| g.len() >= MIN_SAMPLES)
            .map(|((_, in_run), g)| {
                let mut miss = Vec::with_capacity(HORIZON);
                let mut observed = Vec::with_capacity(HORIZON);
                for j in 0..HORIZON {
                    let with = g.iter().filter(|s| s.missed.len() > j).count();
                    if with == 0 {
                        break;
                    }
                    let missed = g.iter().filter(|s| s.missed.get(j) == Some(&true)).count();
                    miss.push(missed as f64 / with as f64);
                    observed.push(with as u32);
                }
                StatusCurve {
                    status: g[0].status,
                    in_run,
                    samples: g.len() as u32,
                    miss,
                    observed,
                }
            })
            .collect();
        StatusTable {
            model: STATUS_MODEL_VERSION,
            lines: samples.len() as u32,
            curves,
        }
    }

    /// The curve of a status in a state, when enough lines back it.
    pub fn curve(&self, status: AbsenceStatus, in_run: bool) -> Option<&StatusCurve> {
        self.curves
            .iter()
            .find(|c| c.status == status && c.in_run == in_run)
    }
}

fn status_key(s: AbsenceStatus) -> u8 {
    match s {
        AbsenceStatus::Out => 0,
        AbsenceStatus::OutForSeason => 1,
        AbsenceStatus::Absent => 2,
        AbsenceStatus::Doubtful => 3,
        AbsenceStatus::Questionable => 4,
        AbsenceStatus::DayToDay => 5,
        AbsenceStatus::Probable => 6,
        AbsenceStatus::Available => 7,
        AbsenceStatus::Unknown => 8,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ids::SeasonYear;
    use crate::domain::models::{EloSource, ResultSource};

    fn game(id: i64, day: u32, home: i32, away: i32) -> Game {
        Game {
            id,
            season: SeasonYear::of(2026),
            game_date: NaiveDate::from_ymd_opt(2025, 11, 1).unwrap()
                + chrono::Duration::days(i64::from(day)),
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

    fn d(day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2025, 11, 1).unwrap() + chrono::Duration::days(i64::from(day))
    }

    fn report(day: u32, player: i32, status: AbsenceStatus) -> ReportedStatus {
        ReportedStatus {
            report_date: d(day),
            player_id: PlayerId::new(player),
            team_id: TeamId::new(1),
            status,
        }
    }

    /// Team 1 plays every second day for ten games. Player 11 misses games 3, 4 and 5
    /// (days 6, 8, 10); player 12 plays every game; player 13 never plays for team 1;
    /// player 14 first plays in game 3.
    fn fixture() -> (Vec<Game>, Vec<BoxLine>) {
        let mut games = Vec::new();
        let mut lines = Vec::new();
        for k in 0..10u32 {
            let id = i64::from(k) + 1;
            games.push(game(id, 2 * k, 1, 2));
            if !(3..=5).contains(&(k + 1)) {
                lines.push(line(id, 11, 1, 30));
            }
            lines.push(line(id, 12, 1, 30));
            if k >= 2 {
                lines.push(line(id, 14, 1, 20));
            }
            lines.push(line(id, 21, 2, 30));
        }
        (games, lines)
    }

    #[test]
    fn a_line_is_followed_through_the_teams_next_games() {
        let (games, lines) = fixture();
        let reports = vec![
            // The morning of game 3: not yet in a run, misses three, then plays.
            report(4, 11, AbsenceStatus::DayToDay),
            // The morning of game 4: in a run (he missed game 3).
            report(6, 11, AbsenceStatus::Out),
            // Listed but plays tonight.
            report(4, 12, AbsenceStatus::Questionable),
            // Never plays for the team: not yet out, misses every game followed.
            report(4, 13, AbsenceStatus::Out),
            // No game played before the report: not yet out, though he missed game 1.
            report(1, 14, AbsenceStatus::Out),
            // A status that takes nothing off, out for the season, and a day after the
            // last game: no sample.
            report(4, 12, AbsenceStatus::Available),
            report(4, 11, AbsenceStatus::OutForSeason),
            report(19, 12, AbsenceStatus::Out),
        ];
        let s = samples(&games, &lines, &reports);
        assert_eq!(s.len(), 5);
        assert_eq!(s[0].status, AbsenceStatus::DayToDay);
        assert!(!s[0].in_run);
        assert_eq!(
            s[0].missed,
            vec![true, true, true, false, false, false, false, false]
        );
        assert!(s[1].in_run);
        assert_eq!(s[1].missed[..3], [true, true, false]);
        assert_eq!(s[1].missed.len(), 7);
        assert!(!s[2].missed[0]);
        assert!(!s[3].in_run);
        assert_eq!(s[3].missed, vec![true; 8]);
        assert!(!s[4].in_run);
        assert_eq!(s[4].missed[..3], [true, false, false]);
    }

    #[test]
    fn the_curve_gives_the_weight_the_expected_games_and_the_span() {
        let mut samples = Vec::new();
        // Sixty day-to-day lines: forty miss tonight only, twenty miss three games; ten
        // of those were cut off after two games.
        for i in 0..60 {
            let missed = if i < 40 {
                vec![true, false, false]
            } else if i < 50 {
                vec![true, true, true, false]
            } else {
                vec![true, true]
            };
            samples.push(StatusSample {
                status: AbsenceStatus::DayToDay,
                in_run: false,
                missed,
            });
        }
        // Too few of another status.
        for _ in 0..10 {
            samples.push(StatusSample {
                status: AbsenceStatus::Out,
                in_run: true,
                missed: vec![true; 5],
            });
        }
        let t = StatusTable::fit(&samples);
        assert_eq!(t.lines, 70);
        assert_eq!(t.curves.len(), 1);
        let c = t.curve(AbsenceStatus::DayToDay, false).unwrap();
        assert!(t.curve(AbsenceStatus::Out, true).is_none());
        assert_eq!(c.samples, 60);
        assert_eq!(c.observed, vec![60, 60, 50, 10]);
        assert!((c.weight() - 1.0).abs() < 1e-12);
        assert!((c.miss[1] - 20.0 / 60.0).abs() < 1e-12);
        assert!((c.miss[2] - 10.0 / 50.0).abs() < 1e-12);
        assert_eq!(c.miss[3], 0.0);
        let expected = 1.0 + 20.0 / 60.0 + 10.0 / 50.0;
        assert!((c.expected_missed() - expected).abs() < 1e-12);
        assert!((c.span_games() - expected).abs() < 1e-12);
        // A small weight: the span stops at the horizon.
        let rare = StatusCurve {
            status: AbsenceStatus::Probable,
            in_run: false,
            samples: 100,
            miss: vec![0.01, 0.5, 0.5],
            observed: vec![100, 100, 100],
        };
        assert_eq!(rare.span_games(), HORIZON as f64);
        let never = StatusCurve {
            status: AbsenceStatus::Probable,
            in_run: false,
            samples: 100,
            miss: vec![0.0, 0.0],
            observed: vec![100, 100],
        };
        assert_eq!((never.weight(), never.span_games()), (0.0, 0.0));
    }
}
