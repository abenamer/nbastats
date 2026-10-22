//! Elo records: the best and worst ratings in the game log, grouped several ways.
//!
//! Pure: `compute` takes per-team-season statistics and the largest single-game swings and
//! returns the record tables the Records page shows. The store adapter gathers the inputs.

use chrono::NaiveDate;
use serde::Serialize;

use crate::domain::ids::SeasonYear;
use crate::domain::models::{Conference, PlayoffResult};
use crate::domain::reference;

/// Games a team must have played in a season before its conference or division counts.
pub const MIN_GAMES_FOR_GROUPS: i64 = 60;
pub const LIST_LEN: usize = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum League {
    /// The NBA and its predecessor, the BAA (1946-47 to 1948-49).
    Nba,
    /// The ABA (1967-68 to 1975-76).
    Aba,
}

impl League {
    pub fn of_code(code: &str) -> League {
        if reference::is_aba_code(code) {
            League::Aba
        } else {
            League::Nba
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            League::Nba => "NBA/BAA",
            League::Aba => "ABA",
        }
    }
}

/// One team's season as the records need it: ratings, extremes with dates, and the result.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TeamSeasonStat {
    pub season: SeasonYear,
    pub code: String,
    pub name: String,
    pub franchise_key: String,
    pub games: i64,
    pub peak: f64,
    pub peak_date: NaiveDate,
    pub low: f64,
    pub low_date: NaiveDate,
    pub wins: i32,
    pub losses: i32,
    pub elo_start: Option<f64>,
    pub elo_pre_playoffs: Option<f64>,
    pub elo_end: Option<f64>,
    pub playoff_result: PlayoffResult,
    pub conference: Option<Conference>,
    pub division: Option<String>,
}

/// The largest rating change in one game, from the winner's side.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GameSwing {
    pub season: SeasonYear,
    pub game_date: NaiveDate,
    pub code: String,
    pub name: String,
    pub opponent_code: String,
    pub points: i32,
    pub opponent_points: i32,
    pub home: bool,
    pub playoff: bool,
    pub elo_pre: f64,
    pub elo_post: f64,
    pub opponent_elo_pre: f64,
}

/// One record line: a team-season and the value that earns it the line.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Entry {
    pub season: SeasonYear,
    pub code: String,
    pub name: String,
    pub franchise_key: String,
    pub league: League,
    pub value: f64,
    pub date: Option<NaiveDate>,
    pub wins: i32,
    pub losses: i32,
    pub playoff_result: PlayoffResult,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Extremes {
    pub best: Entry,
    pub worst: Entry,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DecadeRecord {
    /// First year of the decade of the season's start year (1959-60 belongs to 1950).
    pub decade: i32,
    pub teams: usize,
    pub extremes: Extremes,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SizeRecord {
    pub league: League,
    pub teams: usize,
    pub first_season: SeasonYear,
    pub last_season: SeasonYear,
    pub seasons: usize,
    pub extremes: Extremes,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LeagueRecord {
    pub league: League,
    pub first_season: SeasonYear,
    pub last_season: SeasonYear,
    pub extremes: Extremes,
}

/// A conference or division in one season, rated by the mean pre-playoff Elo of its teams.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GroupEntry {
    pub season: SeasonYear,
    pub conference: Conference,
    pub division: Option<String>,
    pub teams: usize,
    pub mean_elo: f64,
    pub best_code: String,
    pub best_elo: f64,
    pub worst_code: String,
    pub worst_elo: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GroupRecords {
    pub best: Vec<GroupEntry>,
    pub worst: Vec<GroupEntry>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Records {
    pub all_time_high: Vec<Entry>,
    pub all_time_low: Vec<Entry>,
    pub by_decade: Vec<DecadeRecord>,
    pub by_league: Vec<LeagueRecord>,
    pub by_league_size: Vec<SizeRecord>,
    pub best_end: Vec<Entry>,
    pub worst_end: Vec<Entry>,
    pub biggest_rise: Vec<Entry>,
    pub biggest_fall: Vec<Entry>,
    pub best_champion: Vec<Entry>,
    pub worst_champion: Vec<Entry>,
    pub best_uncrowned: Vec<Entry>,
    pub conferences: GroupRecords,
    pub divisions: GroupRecords,
    pub swings: Vec<GameSwing>,
}

fn entry(s: &TeamSeasonStat, value: f64, date: Option<NaiveDate>) -> Entry {
    Entry {
        season: s.season,
        code: s.code.clone(),
        name: s.name.clone(),
        franchise_key: s.franchise_key.clone(),
        league: League::of_code(&s.code),
        value,
        date,
        wins: s.wins,
        losses: s.losses,
        playoff_result: s.playoff_result,
    }
}

fn peak(s: &TeamSeasonStat) -> Entry {
    entry(s, s.peak, Some(s.peak_date))
}

fn low(s: &TeamSeasonStat) -> Entry {
    entry(s, s.low, Some(s.low_date))
}

fn top<'a>(
    stats: impl Iterator<Item = &'a TeamSeasonStat>,
    value: impl Fn(&TeamSeasonStat) -> Option<Entry>,
    descending: bool,
    n: usize,
) -> Vec<Entry> {
    let mut v: Vec<Entry> = stats.filter_map(value).collect();
    v.sort_by(|a, b| {
        let o = a.value.total_cmp(&b.value);
        if descending { o.reverse() } else { o }.then(a.season.cmp(&b.season))
    });
    v.truncate(n);
    v
}

fn extremes(stats: &[&TeamSeasonStat]) -> Option<Extremes> {
    let best = stats
        .iter()
        .max_by(|a, b| a.peak.total_cmp(&b.peak).then(b.season.cmp(&a.season)))?;
    let worst = stats
        .iter()
        .min_by(|a, b| a.low.total_cmp(&b.low).then(b.season.cmp(&a.season)))?;
    Some(Extremes {
        best: peak(best),
        worst: low(worst),
    })
}

/// Decade of the season's start year: 1959-60 (end year 1960) belongs to the 1950s.
pub fn decade_of(season: SeasonYear) -> i32 {
    ((season.end_year() - 1) / 10) * 10
}

fn group_records(
    stats: &[TeamSeasonStat],
    key: impl Fn(&TeamSeasonStat) -> Option<(Conference, Option<String>)>,
) -> GroupRecords {
    use std::collections::BTreeMap;
    // Keyed by the conference name because the enum has no ordering of its own.
    type GroupKey = (SeasonYear, &'static str, Option<String>);
    let mut groups: BTreeMap<GroupKey, (Conference, Vec<&TeamSeasonStat>)> = BTreeMap::new();
    for s in stats {
        if s.games < MIN_GAMES_FOR_GROUPS || s.elo_pre_playoffs.is_none() {
            continue;
        }
        if let Some((c, d)) = key(s) {
            groups
                .entry((s.season, c.as_str(), d))
                .or_insert((c, vec![]))
                .1
                .push(s);
        }
    }
    let mut entries: Vec<GroupEntry> = groups
        .into_iter()
        .filter(|(_, (_, members))| members.len() >= 2)
        .map(|((season, _, division), (conference, members))| {
            let rated: Vec<(&TeamSeasonStat, f64)> = members
                .iter()
                .map(|m| (*m, m.elo_pre_playoffs.unwrap_or(0.0)))
                .collect();
            let mean = rated.iter().map(|r| r.1).sum::<f64>() / rated.len() as f64;
            let best = rated
                .iter()
                .max_by(|a, b| a.1.total_cmp(&b.1))
                .expect("non-empty");
            let worst = rated
                .iter()
                .min_by(|a, b| a.1.total_cmp(&b.1))
                .expect("non-empty");
            GroupEntry {
                season,
                conference,
                division,
                teams: rated.len(),
                mean_elo: mean,
                best_code: best.0.code.clone(),
                best_elo: best.1,
                worst_code: worst.0.code.clone(),
                worst_elo: worst.1,
            }
        })
        .collect();
    entries.sort_by(|a, b| b.mean_elo.total_cmp(&a.mean_elo));
    let best: Vec<GroupEntry> = entries.iter().take(LIST_LEN).cloned().collect();
    let worst: Vec<GroupEntry> = entries.iter().rev().take(LIST_LEN).cloned().collect();
    GroupRecords { best, worst }
}

/// Build every record table from the team-season statistics and the largest game swings.
pub fn compute(stats: &[TeamSeasonStat], swings: Vec<GameSwing>) -> Records {
    use std::collections::BTreeMap;
    let all = || stats.iter();

    // Decades
    let mut by_decade_map: BTreeMap<i32, Vec<&TeamSeasonStat>> = BTreeMap::new();
    for s in stats {
        by_decade_map
            .entry(decade_of(s.season))
            .or_default()
            .push(s);
    }
    let by_decade = by_decade_map
        .into_iter()
        .filter_map(|(decade, members)| {
            Some(DecadeRecord {
                decade,
                teams: members.len(),
                extremes: extremes(&members)?,
            })
        })
        .collect();

    // Leagues
    let mut by_league_map: BTreeMap<&'static str, (League, Vec<&TeamSeasonStat>)> = BTreeMap::new();
    for s in stats {
        let l = League::of_code(&s.code);
        by_league_map
            .entry(l.label())
            .or_insert((l, vec![]))
            .1
            .push(s);
    }
    let mut by_league: Vec<LeagueRecord> = by_league_map
        .into_values()
        .filter_map(|(league, members)| {
            Some(LeagueRecord {
                league,
                first_season: members.iter().map(|m| m.season).min()?,
                last_season: members.iter().map(|m| m.season).max()?,
                extremes: extremes(&members)?,
            })
        })
        .collect();
    by_league.sort_by_key(|r| r.first_season);

    // League sizes: number of teams of that league in that season.
    let mut sizes: BTreeMap<(SeasonYear, League), usize> = BTreeMap::new();
    for s in stats {
        *sizes
            .entry((s.season, League::of_code(&s.code)))
            .or_default() += 1;
    }
    let mut by_size_map: BTreeMap<(usize, &'static str), (League, Vec<&TeamSeasonStat>)> =
        BTreeMap::new();
    for s in stats {
        let l = League::of_code(&s.code);
        let n = sizes[&(s.season, l)];
        by_size_map
            .entry((n, l.label()))
            .or_insert((l, vec![]))
            .1
            .push(s);
    }
    let by_league_size = by_size_map
        .into_iter()
        .filter_map(|((teams, _), (league, members))| {
            let mut seasons: Vec<SeasonYear> = members.iter().map(|m| m.season).collect();
            seasons.sort_unstable();
            seasons.dedup();
            Some(SizeRecord {
                league,
                teams,
                first_season: *seasons.first()?,
                last_season: *seasons.last()?,
                seasons: seasons.len(),
                extremes: extremes(&members)?,
            })
        })
        .collect();

    let champions = || {
        stats
            .iter()
            .filter(|s| s.playoff_result == PlayoffResult::Champion)
    };
    let others = || {
        stats
            .iter()
            .filter(|s| s.playoff_result != PlayoffResult::Champion)
    };
    let full = |s: &TeamSeasonStat| s.games >= MIN_GAMES_FOR_GROUPS;
    let rise = |s: &TeamSeasonStat| {
        let delta = s.elo_end? - s.elo_start?;
        full(s).then(|| entry(s, delta, None))
    };
    let end = |s: &TeamSeasonStat| {
        let e = s.elo_end?;
        full(s).then(|| entry(s, e, None))
    };

    Records {
        all_time_high: top(all(), |s| Some(peak(s)), true, LIST_LEN),
        all_time_low: top(all(), |s| Some(low(s)), false, LIST_LEN),
        by_decade,
        by_league,
        by_league_size,
        best_end: top(all(), end, true, LIST_LEN),
        worst_end: top(all(), end, false, LIST_LEN),
        biggest_rise: top(all(), rise, true, LIST_LEN),
        biggest_fall: top(all(), rise, false, LIST_LEN),
        best_champion: top(champions(), |s| Some(peak(s)), true, LIST_LEN),
        worst_champion: top(champions(), |s| Some(peak(s)), false, LIST_LEN),
        best_uncrowned: top(others(), |s| Some(peak(s)), true, LIST_LEN),
        conferences: group_records(stats, |s| s.conference.map(|c| (c, None))),
        divisions: group_records(stats, |s| Some((s.conference?, Some(s.division.clone()?)))),
        swings,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    fn stat(season: i32, code: &str, peak: f64, low: f64, result: PlayoffResult) -> TeamSeasonStat {
        TeamSeasonStat {
            season: SeasonYear::of(season),
            code: code.to_string(),
            name: format!("Team {code}"),
            franchise_key: code.to_string(),
            games: 82,
            peak,
            peak_date: d("2017-06-07"),
            low,
            low_date: d("2016-10-25"),
            wins: 50,
            losses: 32,
            elo_start: Some(1500.0),
            elo_pre_playoffs: Some(peak - 20.0),
            elo_end: Some(peak - 10.0),
            playoff_result: result,
            conference: Some(if code.starts_with('W') {
                Conference::West
            } else {
                Conference::East
            }),
            division: Some(
                if code.starts_with('W') {
                    "Pacific"
                } else {
                    "Atlantic"
                }
                .to_string(),
            ),
        }
    }

    #[test]
    fn decade_uses_the_start_year() {
        assert_eq!(decade_of(SeasonYear::of(1960)), 1950);
        assert_eq!(decade_of(SeasonYear::of(1961)), 1960);
        assert_eq!(decade_of(SeasonYear::of(2017)), 2010);
    }

    #[test]
    fn league_follows_the_aba_code_list() {
        assert_eq!(League::of_code("KEN"), League::Aba);
        assert_eq!(League::of_code("INA"), League::Aba);
        assert_eq!(League::of_code("IND"), League::Nba);
        assert_eq!(League::of_code("GSW"), League::Nba);
    }

    #[test]
    fn records_pick_the_right_rows() {
        let stats = vec![
            stat(2017, "WGS", 1865.0, 1700.0, PlayoffResult::Champion),
            stat(2017, "ECL", 1750.0, 1600.0, PlayoffResult::Finals),
            stat(2016, "WGS", 1839.0, 1650.0, PlayoffResult::Finals),
            stat(2016, "ECL", 1720.0, 1550.0, PlayoffResult::Champion),
            stat(1976, "KEN", 1700.0, 1400.0, PlayoffResult::Missed),
            stat(1976, "WGS", 1600.0, 1300.0, PlayoffResult::Champion),
        ];
        let r = compute(&stats, vec![]);
        assert_eq!(r.all_time_high[0].value, 1865.0);
        assert_eq!(r.all_time_high[0].code, "WGS");
        assert_eq!(r.all_time_low[0].value, 1300.0);
        assert_eq!(r.by_decade.len(), 2);
        assert_eq!(r.by_decade[0].decade, 1970);
        assert_eq!(r.by_decade[1].extremes.best.value, 1865.0);
        assert_eq!(r.by_league.len(), 2);
        let aba = r
            .by_league
            .iter()
            .find(|l| l.league == League::Aba)
            .unwrap();
        assert_eq!(aba.extremes.best.code, "KEN");
        // 2016 and 2017 each have two NBA teams; 1976 has one NBA and one ABA team.
        assert!(r
            .by_league_size
            .iter()
            .any(|s| s.teams == 2 && s.seasons == 2));
        assert_eq!(r.best_champion[0].value, 1865.0);
        assert_eq!(r.worst_champion[0].value, 1600.0);
        assert_eq!(r.best_uncrowned[0].value, 1839.0);
        assert_eq!(r.biggest_rise[0].value, 1855.0 - 1500.0);
        // Conference tables: 2017 West (WGS alone) has one team, so it is skipped; no group
        // reaches two teams here.
        assert!(r.conferences.best.is_empty());
    }

    #[test]
    fn conference_and_division_means_need_two_teams_and_full_seasons() {
        let mut stats = vec![
            stat(2017, "WGS", 1865.0, 1700.0, PlayoffResult::Champion),
            stat(2017, "WLA", 1500.0, 1400.0, PlayoffResult::Missed),
            stat(2017, "EBO", 1600.0, 1500.0, PlayoffResult::ConfFinals),
            stat(2017, "ECL", 1750.0, 1600.0, PlayoffResult::Finals),
        ];
        stats[1].games = 10; // an unfinished season does not count
        let r = compute(&stats, vec![]);
        assert_eq!(r.conferences.best.len(), 1);
        let east = &r.conferences.best[0];
        assert_eq!(east.conference, Conference::East);
        assert_eq!(east.teams, 2);
        assert!((east.mean_elo - (1580.0 + 1730.0) / 2.0).abs() < 1e-9);
        assert_eq!(
            (east.best_code.as_str(), east.worst_code.as_str()),
            ("ECL", "EBO")
        );
        assert_eq!(r.divisions.best[0].division.as_deref(), Some("Atlantic"));
    }
}
