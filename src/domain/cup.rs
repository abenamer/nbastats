//! The NBA Cup (the in-season tournament, from 2023-24): the groups, the group standings
//! with the Cup's tiebreaks, the knockout seeding and the exact chances through a knockout
//! bracket.
//!
//! The only curated fact per season is its dates: the group nights, the quarterfinal and
//! semifinal dates and the final. The groups themselves follow from the schedule: on the
//! group nights every game is a group game, each team plays its four group-mates once, so
//! the games on those nights split the league into six groups of five. The group-stage
//! games are regular-season games; the quarterfinals and semifinals count in the standings
//! too; the final does not and is stored as its own game type.

use std::collections::{HashMap, HashSet};

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

use crate::domain::ids::SeasonYear;
use crate::domain::models::Conference;

/// The dates of one Cup, curated from the league's announcements. Group games are the
/// regular-season games played on the group nights.
#[derive(Debug, Clone, Copy)]
pub struct CupDates {
    pub season: SeasonYear,
    pub group_nights: &'static [&'static str],
    pub quarterfinals: &'static [&'static str],
    pub semifinals: &'static str,
    pub final_date: &'static str,
}

/// The Cups with dates in the application. A season missing here has no Cup forecast until
/// its dates are added; the dates need a check against the league's calendar.
pub const CUP_SEASONS: &[CupDates] = &[
    CupDates {
        season: SeasonYear::of(2024),
        group_nights: &[
            "2023-11-03",
            "2023-11-10",
            "2023-11-14",
            "2023-11-17",
            "2023-11-21",
            "2023-11-24",
            "2023-11-28",
        ],
        quarterfinals: &["2023-12-04", "2023-12-05"],
        semifinals: "2023-12-07",
        final_date: "2023-12-09",
    },
    CupDates {
        season: SeasonYear::of(2025),
        group_nights: &[
            "2024-11-12",
            "2024-11-15",
            "2024-11-19",
            "2024-11-22",
            "2024-11-26",
            "2024-11-29",
            "2024-12-03",
        ],
        quarterfinals: &["2024-12-10", "2024-12-11"],
        semifinals: "2024-12-14",
        final_date: "2024-12-17",
    },
    CupDates {
        season: SeasonYear::of(2026),
        group_nights: &[
            "2025-10-31",
            "2025-11-07",
            "2025-11-14",
            "2025-11-21",
            "2025-11-25",
            "2025-11-26",
            "2025-11-28",
        ],
        quarterfinals: &["2025-12-09", "2025-12-10"],
        semifinals: "2025-12-13",
        final_date: "2025-12-16",
    },
];

pub const GROUP_SIZE: usize = 5;
pub const GROUPS_PER_CONFERENCE: usize = 3;
/// Teams of a conference in the knockout: the three group winners and one wild card.
pub const KNOCKOUT_PER_CONFERENCE: usize = 4;
/// Points of margin a simulated group game expects per Elo point of edge: FiveThirtyEight's
/// conversion of 28 Elo points per point.
pub const POINTS_PER_ELO: f64 = 1.0 / 28.0;
/// The spread of a simulated margin around its expectation, in points: the usual spread
/// of an NBA margin around the pre-game line.
pub const MARGIN_SD: f64 = 12.0;

fn date(s: &str) -> NaiveDate {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").expect("curated date")
}

/// The dates of a season's Cup, parsed; `None` for a season without one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dates {
    pub group_nights: Vec<NaiveDate>,
    pub quarterfinals: Vec<NaiveDate>,
    pub semifinals: NaiveDate,
    pub final_date: NaiveDate,
}

impl Dates {
    pub fn knockout_dates(&self) -> Vec<NaiveDate> {
        let mut v = self.quarterfinals.clone();
        v.push(self.semifinals);
        v.push(self.final_date);
        v
    }
}

/// Regular-season games per team in a full NBA season.
pub const GAMES_PER_TEAM: usize = 82;

/// How many regular-season games a team's stored schedule still lacks in a Cup season: the
/// league publishes 80 games per team and adds the two Cup-week games in December, once the
/// knockout field is known. `None` outside the Cup era or for a full schedule.
pub fn games_to_be_scheduled(season: SeasonYear, on_schedule: usize) -> Option<usize> {
    if season < CUP_SEASONS[0].season || on_schedule >= GAMES_PER_TEAM {
        return None;
    }
    Some(GAMES_PER_TEAM - on_schedule)
}

/// The sentence a tool result carries when a Cup-season schedule is short, or nothing.
pub fn schedule_note(season: SeasonYear, on_schedule: usize) -> Option<String> {
    games_to_be_scheduled(season, on_schedule).map(|n| {
        format!(
            "The stored schedule holds {on_schedule} regular-season games; the league adds {n} NBA Cup-week game(s) in December, once the knockout field is known."
        )
    })
}

pub fn dates_for(season: SeasonYear) -> Option<Dates> {
    CUP_SEASONS
        .iter()
        .find(|c| c.season == season)
        .map(|c| Dates {
            group_nights: c.group_nights.iter().map(|d| date(d)).collect(),
            quarterfinals: c.quarterfinals.iter().map(|d| date(d)).collect(),
            semifinals: date(c.semifinals),
            final_date: date(c.final_date),
        })
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CupError {
    #[error("the games on the group nights do not form six groups of five: {0}")]
    Shape(String),
}

/// A group: a conference and its five teams, by team index, in index order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Group {
    pub conference: Conference,
    pub teams: Vec<usize>,
}

/// The groups the games on the group nights make: each team plays four distinct
/// group-mates of its conference once, and the pairs connect the league into six groups of
/// five, three per conference. `games` are (home, away) by team index.
pub fn groups_from_games(
    conf: &[Conference],
    games: &[(usize, usize)],
) -> Result<Vec<Group>, CupError> {
    let n = conf.len();
    let mut adjacent: Vec<HashSet<usize>> = vec![HashSet::new(); n];
    for &(h, a) in games {
        if h >= n || a >= n || h == a {
            return Err(CupError::Shape(format!("game between {h} and {a}")));
        }
        if conf[h] != conf[a] {
            return Err(CupError::Shape(format!(
                "a game across conferences between teams {h} and {a}"
            )));
        }
        if !adjacent[h].insert(a) || !adjacent[a].insert(h) {
            return Err(CupError::Shape(format!(
                "teams {h} and {a} meet twice on the group nights"
            )));
        }
    }
    if let Some(t) = (0..n).find(|&t| adjacent[t].len() != GROUP_SIZE - 1) {
        return Err(CupError::Shape(format!(
            "team {t} plays {} group games instead of {}",
            adjacent[t].len(),
            GROUP_SIZE - 1
        )));
    }
    let mut seen = vec![false; n];
    let mut groups = Vec::new();
    for start in 0..n {
        if seen[start] {
            continue;
        }
        let mut members = Vec::new();
        let mut stack = vec![start];
        while let Some(t) = stack.pop() {
            if seen[t] {
                continue;
            }
            seen[t] = true;
            members.push(t);
            stack.extend(adjacent[t].iter().copied().filter(|&o| !seen[o]));
        }
        if members.len() != GROUP_SIZE {
            return Err(CupError::Shape(format!(
                "a group of {} teams around team {start}",
                members.len()
            )));
        }
        members.sort_unstable();
        groups.push(Group {
            conference: conf[start],
            teams: members,
        });
    }
    for c in [Conference::East, Conference::West] {
        let count = groups.iter().filter(|g| g.conference == c).count();
        if count != GROUPS_PER_CONFERENCE {
            return Err(CupError::Shape(format!(
                "{count} groups in the {c:?} instead of {GROUPS_PER_CONFERENCE}"
            )));
        }
    }
    groups.sort_by_key(|g| (g.conference.index(), g.teams[0]));
    Ok(groups)
}

/// A decided group game: the margin is the winner's, positive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GroupResult {
    pub home: usize,
    pub away: usize,
    pub home_won: bool,
    pub margin: i32,
}

impl GroupResult {
    pub fn winner(&self) -> usize {
        if self.home_won {
            self.home
        } else {
            self.away
        }
    }
    pub fn loser(&self) -> usize {
        if self.home_won {
            self.away
        } else {
            self.home
        }
    }
}

/// A team's line in its group table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct GroupLine {
    pub team: usize,
    pub wins: i32,
    pub losses: i32,
    /// Point differential over the group games.
    pub diff: i32,
}

/// The group table from its decided games: wins first, then the record among the tied
/// teams, then the point differential, then the random key. (The league also counts points
/// scored and the previous season's record before its drawing; the simulation has neither.)
pub fn group_table(
    group: &[usize],
    results: &[GroupResult],
    key: impl Fn(usize) -> u64,
) -> Vec<GroupLine> {
    let in_group: HashSet<usize> = group.iter().copied().collect();
    let mut lines: HashMap<usize, GroupLine> = group
        .iter()
        .map(|&t| {
            (
                t,
                GroupLine {
                    team: t,
                    wins: 0,
                    losses: 0,
                    diff: 0,
                },
            )
        })
        .collect();
    let mut h2h: HashMap<(usize, usize), i32> = HashMap::new();
    for r in results
        .iter()
        .filter(|r| in_group.contains(&r.home) && in_group.contains(&r.away))
    {
        let (w, l) = (r.winner(), r.loser());
        lines.get_mut(&w).expect("member").wins += 1;
        lines.get_mut(&l).expect("member").losses += 1;
        lines.get_mut(&w).expect("member").diff += r.margin;
        lines.get_mut(&l).expect("member").diff -= r.margin;
        *h2h.entry((w, l)).or_insert(0) += 1;
    }
    let mut ordered: Vec<GroupLine> = lines.into_values().collect();
    ordered.sort_by_key(|l| std::cmp::Reverse(l.wins));
    // Each run of equal wins follows the league's procedure: rank the tied teams by their
    // record against each other, then differential, then key; take the top team out and
    // restart with the teams that remain, whose record against each other is recomputed.
    let mut out = Vec::with_capacity(ordered.len());
    let mut i = 0;
    while i < ordered.len() {
        let mut j = i;
        while j < ordered.len() && ordered[j].wins == ordered[i].wins {
            j += 1;
        }
        let mut tied: Vec<GroupLine> = ordered[i..j].to_vec();
        while !tied.is_empty() {
            let among = |t: usize| -> i32 {
                tied.iter()
                    .map(|o| h2h.get(&(t, o.team)).copied().unwrap_or(0))
                    .sum()
            };
            let best = (0..tied.len())
                .max_by(|&a, &b| {
                    let (la, lb) = (&tied[a], &tied[b]);
                    among(la.team)
                        .cmp(&among(lb.team))
                        .then(la.diff.cmp(&lb.diff))
                        .then(key(la.team).cmp(&key(lb.team)))
                })
                .expect("non-empty");
            out.push(tied.remove(best));
        }
        i = j;
    }
    out
}

/// The knockout seeds of a conference, 1 to 4: the three group winners by wins, then
/// differential, then key; the wild card is the best runner-up by the same order.
pub fn knockout_seeds(
    tables: &[(Conference, Vec<GroupLine>)],
    conference: Conference,
    key: impl Fn(usize) -> u64,
) -> Option<[usize; KNOCKOUT_PER_CONFERENCE]> {
    let order = |a: &GroupLine, b: &GroupLine| {
        b.wins
            .cmp(&a.wins)
            .then(b.diff.cmp(&a.diff))
            .then(key(b.team).cmp(&key(a.team)))
    };
    let mine: Vec<&Vec<GroupLine>> = tables
        .iter()
        .filter(|(c, _)| *c == conference)
        .map(|(_, t)| t)
        .collect();
    if mine.len() != GROUPS_PER_CONFERENCE || mine.iter().any(|t| t.len() < 2) {
        return None;
    }
    let mut winners: Vec<GroupLine> = mine.iter().map(|t| t[0]).collect();
    winners.sort_by(order);
    let mut seconds: Vec<GroupLine> = mine.iter().map(|t| t[1]).collect();
    seconds.sort_by(order);
    Some([
        winners[0].team,
        winners[1].team,
        winners[2].team,
        seconds[0].team,
    ])
}

/// Slot order of a knockout bracket: per conference, seed 1, seed 4, seed 2, seed 3, so
/// slots pair up into the quarterfinals (0-1, 2-3 in the East; 4-5, 6-7 in the West).
pub const CUP_SLOT_SEEDS: [u8; 4] = [1, 4, 2, 3];
pub const CUP_SLOTS: usize = 8;

/// Slots from a conference's seeds in seed order.
pub fn slots_of(east: [usize; 4], west: [usize; 4]) -> [usize; CUP_SLOTS] {
    let mut out = [0; CUP_SLOTS];
    for (i, &seed) in CUP_SLOT_SEEDS.iter().enumerate() {
        out[i] = east[seed as usize - 1];
        out[4 + i] = west[seed as usize - 1];
    }
    out
}

/// Whether slot `a` hosts its quarterfinal against slot `b`: the higher seed does.
pub fn hosts_quarterfinal(a: usize, b: usize) -> bool {
    CUP_SLOT_SEEDS[a % 4] < CUP_SLOT_SEEDS[b % 4]
}

/// Exact chances through a knockout bracket of eight slots: per slot, the probability of
/// reaching the semifinal, the final and winning the Cup. `p` gives the probability that
/// the first slot beats the second in one game, with whether the first slot is at home
/// (`Some(true)`), away (`Some(false)`) or on a neutral floor (`None`).
pub fn exact(p: impl Fn(usize, usize, Option<bool>) -> f64) -> [[f64; 3]; CUP_SLOTS] {
    // Quarterfinals: the higher seed hosts.
    let mut qf = [0.0; CUP_SLOTS];
    for (s, q) in qf.iter_mut().enumerate() {
        let o = s ^ 1;
        *q = p(s, o, Some(hosts_quarterfinal(s, o)));
    }
    // Semifinals on a neutral floor: against either winner of the other pair in the
    // conference.
    let mut sf = [0.0; CUP_SLOTS];
    for (s, v) in sf.iter_mut().enumerate() {
        let other_pair = (s / 4) * 4 + (((s % 4) / 2) ^ 1) * 2;
        *v = qf[s]
            * (other_pair..other_pair + 2)
                .map(|o| qf[o] * p(s, o, None))
                .sum::<f64>();
    }
    // The final on a neutral floor: against either finalist of the other conference.
    let mut out = [[0.0; 3]; CUP_SLOTS];
    for (s, row) in out.iter_mut().enumerate() {
        let other_conf = if s < 4 { 4..8 } else { 0..4 };
        let win = sf[s] * other_conf.map(|o| sf[o] * p(s, o, None)).sum::<f64>();
        *row = [qf[s], sf[s], win];
    }
    out
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn a_cup_season_schedule_short_of_82_names_the_games_to_come() {
        assert_eq!(games_to_be_scheduled(SeasonYear::of(2027), 80), Some(2));
        assert_eq!(games_to_be_scheduled(SeasonYear::of(2027), 82), None);
        assert_eq!(games_to_be_scheduled(SeasonYear::of(2027), 83), None);
        // Before the Cup a short schedule is just a short schedule.
        assert_eq!(games_to_be_scheduled(SeasonYear::of(2016), 80), None);
        assert!(schedule_note(SeasonYear::of(2027), 80)
            .unwrap()
            .contains("adds 2 NBA Cup-week"));
        assert_eq!(schedule_note(SeasonYear::of(2027), 82), None);
    }

    fn conferences() -> Vec<Conference> {
        (0..30)
            .map(|i| {
                if i < 15 {
                    Conference::East
                } else {
                    Conference::West
                }
            })
            .collect()
    }

    /// Group games for six groups of five: every pair within each block of five plays once.
    fn group_games() -> Vec<(usize, usize)> {
        let mut games = Vec::new();
        for g in 0..6 {
            let base = g * 5;
            for a in 0..5 {
                for b in a + 1..5 {
                    games.push((base + a, base + b));
                }
            }
        }
        games
    }

    #[test]
    fn the_groups_follow_from_the_games_on_the_group_nights() {
        let groups = groups_from_games(&conferences(), &group_games()).unwrap();
        assert_eq!(groups.len(), 6);
        assert_eq!(groups[0].teams, [0, 1, 2, 3, 4]);
        assert_eq!(groups[3].conference, Conference::West);
        assert_eq!(groups[5].teams, [25, 26, 27, 28, 29]);
        // A missing game, a repeated pair, a cross-conference game: no groups.
        let mut games = group_games();
        games.pop();
        assert!(groups_from_games(&conferences(), &games).is_err());
        let mut games = group_games();
        games.push((0, 1));
        assert!(groups_from_games(&conferences(), &games).is_err());
        let mut games = group_games();
        games[0] = (0, 15);
        games.push((1, 2));
        assert!(groups_from_games(&conferences(), &games).is_err());
        // The dates of a season the application knows, and one it does not.
        assert_eq!(
            dates_for(SeasonYear::of(2024)).unwrap().group_nights.len(),
            7
        );
        assert_eq!(
            dates_for(SeasonYear::of(2024))
                .unwrap()
                .knockout_dates()
                .len(),
            4
        );
        assert!(dates_for(SeasonYear::of(2023)).is_none());
    }

    fn result(home: usize, away: usize, home_won: bool, margin: i32) -> GroupResult {
        GroupResult {
            home,
            away,
            home_won,
            margin,
        }
    }

    #[test]
    fn the_group_table_breaks_ties_head_to_head_then_by_differential() {
        // Teams 0 and 1 both finish 3-1; team 1 beat team 0, so team 1 wins the group even
        // with the worse differential. Teams 2 and 4 are 1-3 and team 2 beat team 4.
        let group = [0, 1, 2, 3, 4];
        let results = vec![
            result(1, 0, true, 3),
            result(0, 2, true, 20),
            result(0, 3, true, 20),
            result(0, 4, true, 20),
            result(1, 2, true, 1),
            result(1, 3, true, 1),
            result(4, 1, true, 1),
            result(2, 3, false, 2),
            result(2, 4, true, 30),
            result(3, 4, true, 1),
        ];
        let table = group_table(&group, &results, |t| t as u64);
        let order: Vec<usize> = table.iter().map(|l| l.team).collect();
        assert_eq!(order[..2], [1, 0]);
        assert_eq!((table[0].wins, table[0].losses, table[0].diff), (3, 1, 4));
        assert_eq!((table[1].wins, table[1].diff), (3, 57));
        // 2: beat 4 by 30, lost to 0 by 20, to 1 by 1, to 3 by 2 -> 1-3, diff +7.
        // 3: beat 2 by 2, beat 4 by 1, lost to 0 by 20, to 1 by 1 -> 2-2.
        // 4: beat 1 by 1, lost to 0, 2, 3 -> 1-3, diff 1-20-30-1 = -50.
        assert_eq!(order[2], 3);
        assert_eq!(order[3..], [2, 4]);
        // A table with no games yet is ordered by the key alone, stable for the caller.
        let fresh = group_table(&group, &[], |t| 10 - t as u64);
        assert_eq!(fresh.iter().map(|l| l.team).collect::<Vec<_>>(), group);
    }

    #[test]
    fn a_tie_restarts_among_the_teams_that_remain_and_falls_back_to_the_differential() {
        // Five teams at 2-2 in a circle, so among the five every team has two wins against
        // the others and the differential places E (4) first. Among the four that remain,
        // A (0) and B (1) have two wins each and C (2) and D (3) one each: A is second by
        // differential over B. Among the two that remain, C beat D, so C is fourth though
        // D has the better differential. A single pass by differential would have put B
        // last and D before C.
        let group = [0, 1, 2, 3, 4];
        let results = vec![
            result(0, 1, true, 10),
            result(0, 2, true, 10),
            result(1, 2, true, 1),
            result(1, 3, true, 1),
            result(2, 3, true, 1),
            result(2, 4, true, 1),
            result(3, 4, true, 1),
            result(3, 0, true, 1),
            result(4, 0, true, 1),
            result(4, 1, true, 30),
        ];
        let table = group_table(&group, &results, |t| t as u64);
        assert!(table.iter().all(|l| l.wins == 2 && l.losses == 2));
        let order: Vec<usize> = table.iter().map(|l| l.team).collect();
        assert_eq!(order, [4, 0, 1, 2, 3]);
        assert_eq!(
            table.iter().map(|l| l.diff).collect::<Vec<_>>(),
            [29, 18, -38, -9, 0]
        );
        // Three teams in a circle with equal differential: the key decides the first place,
        // then the two that remain are ordered by their own game.
        let circle = vec![
            result(0, 1, true, 5),
            result(1, 2, true, 5),
            result(2, 0, true, 5),
        ];
        let table = group_table(&[0, 1, 2], &circle, |t| t as u64);
        assert_eq!(table.iter().map(|l| l.team).collect::<Vec<_>>(), [2, 0, 1]);
    }

    #[test]
    fn the_knockout_takes_the_group_winners_and_the_best_runner_up() {
        let line = |team, wins, diff| GroupLine {
            team,
            wins,
            losses: 4 - wins,
            diff,
        };
        let tables = vec![
            (Conference::East, vec![line(0, 4, 30), line(1, 3, 10)]),
            (Conference::East, vec![line(5, 3, 40), line(6, 3, 25)]),
            (Conference::East, vec![line(10, 4, 20), line(11, 2, 5)]),
            (Conference::West, vec![line(15, 3, 1), line(16, 2, 0)]),
            (Conference::West, vec![line(20, 3, 2), line(21, 2, 1)]),
            (Conference::West, vec![line(25, 3, 3), line(26, 2, 2)]),
        ];
        // Winners 0 (4-0, +30), 10 (4-0, +20), 5 (3-1); wild card 6 (3-1, +25).
        assert_eq!(
            knockout_seeds(&tables, Conference::East, |t| t as u64),
            Some([0, 10, 5, 6])
        );
        assert_eq!(
            knockout_seeds(&tables, Conference::West, |t| t as u64),
            Some([25, 20, 15, 26])
        );
        let slots = slots_of([0, 10, 5, 6], [25, 20, 15, 26]);
        assert_eq!(slots, [0, 6, 10, 5, 25, 26, 20, 15]);
        assert!(hosts_quarterfinal(0, 1) && !hosts_quarterfinal(1, 0));
        assert!(hosts_quarterfinal(2, 3) && hosts_quarterfinal(6, 7));
    }

    #[test]
    fn the_exact_chances_through_a_knockout_bracket_add_up() {
        // Coin flips everywhere: each slot reaches the semifinal with 1/2, the final with
        // 1/4 and wins with 1/8.
        let coin = exact(|_, _, _| 0.5);
        for s in coin {
            assert!((s[0] - 0.5).abs() < 1e-12);
            assert!((s[1] - 0.25).abs() < 1e-12);
            assert!((s[2] - 0.125).abs() < 1e-12);
        }
        // A slot that always wins at home and on a neutral floor wins the Cup.
        let sure = exact(|a, b, _| {
            if a == 2 {
                1.0
            } else if b == 2 {
                0.0
            } else {
                0.5
            }
        });
        assert!((sure[2][2] - 1.0).abs() < 1e-12);
        let total: f64 = sure.iter().map(|s| s[2]).sum();
        assert!((total - 1.0).abs() < 1e-12);
        // Home court is asked for in the quarterfinals only, by the higher seed.
        let asked = std::cell::RefCell::new(Vec::new());
        let _ = exact(|a, b, home| {
            asked.borrow_mut().push((a, b, home));
            0.5
        });
        let asked = asked.into_inner();
        assert!(asked
            .iter()
            .any(|&(a, b, h)| (a, b, h) == (0, 1, Some(true))));
        assert!(asked
            .iter()
            .any(|&(a, b, h)| (a, b, h) == (1, 0, Some(false))));
        assert!(asked
            .iter()
            .filter(|&&(_, _, h)| h.is_some())
            .all(|&(a, b, _)| a ^ 1 == b));
    }
}
