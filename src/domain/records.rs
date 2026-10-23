//! Elo records: the best and worst ratings in the game log, grouped several ways.
//!
//! Pure: `compute` takes per-team-season statistics and the largest single-game swings and
//! returns the record tables the Records page shows. The store adapter gathers the inputs.

use chrono::NaiveDate;
use serde::Serialize;

use crate::domain::ids::SeasonYear;
use std::collections::BTreeMap;

use crate::domain::models::{Conference, PlayoffResult};
use crate::domain::reference;

/// Share of a season's median game count a team must reach before its season counts as
/// full. Schedules ran from 48 to 82 games, so the floor follows the season, not a fixed number.
pub const FULL_SEASON_SHARE: f64 = 0.8;
/// The NBA adopted the 3-point line in 1979-80. The ABA had it from its first season.
pub const THREE_POINT_SEASON: SeasonYear = SeasonYear::of(1980);
pub const LIST_LEN: usize = 10;
/// Rows in the greatest and weakest matchup tables.
pub const MATCHUP_LEN: usize = 20;

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

/// A played game ranked by the two teams' combined rating before tip-off: the greatest
/// matchups at the top of the log, the weakest ("clankers") at the bottom.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Matchup {
    pub season: SeasonYear,
    pub game_date: NaiveDate,
    pub home_code: String,
    pub home_name: String,
    pub away_code: String,
    pub away_name: String,
    pub home_elo_pre: f64,
    pub away_elo_pre: f64,
    pub home_score: i32,
    pub away_score: i32,
    pub playoff: bool,
}

impl Matchup {
    pub fn combined_elo(&self) -> f64 {
        self.home_elo_pre + self.away_elo_pre
    }
}

/// Which end of the combined-rating order a matchup query returns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchupOrder {
    Strongest,
    Weakest,
}

/// A champion's road to the title: the mean rating of its opponents before each game,
/// shown for the regular season and for the playoffs (play-in included), and combined
/// over every game.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ChampionRoad {
    pub season: SeasonYear,
    pub code: String,
    pub name: String,
    pub regular_opponent_elo: f64,
    pub regular_games: i64,
    pub playoff_opponent_elo: f64,
    pub playoff_games: i64,
    /// The strongest playoff opponent, by rating before the game.
    pub toughest_opponent_code: String,
    pub toughest_opponent_elo: f64,
    /// The road rating, filled by `compute` from the phases (see [`ChampionRoad::compute_road`]).
    pub road: f64,
}

impl ChampionRoad {
    /// The road rating: the mean opponent rating over every game, so each phase weighs by
    /// its games (pro rata), not by a fixed share.
    pub fn compute_road(&self) -> f64 {
        let games = (self.regular_games + self.playoff_games) as f64;
        if games == 0.0 {
            return 0.0;
        }
        (self.regular_opponent_elo * self.regular_games as f64
            + self.playoff_opponent_elo * self.playoff_games as f64)
            / games
    }
}

/// The weakest game of an era: the clanker. A league-wide list would only find the newest
/// franchises (every team starts at 1300), so the clankers are taken era by era.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EraClanker {
    pub league: League,
    pub first_season: SeasonYear,
    pub last_season: SeasonYear,
    pub teams: usize,
    pub three_point: bool,
    pub game: Matchup,
}

/// The hardest and the easiest road to the title within one era. With one champion in the
/// era, both are that champion.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EraRoads {
    pub league: League,
    pub first_season: SeasonYear,
    pub last_season: SeasonYear,
    pub teams: usize,
    pub three_point: bool,
    pub champions: usize,
    pub hardest: ChampionRoad,
    pub easiest: ChampionRoad,
}

/// The game-level record inputs the store ranks for `compute`.
#[derive(Debug, Clone, Default)]
pub struct GameRecords {
    pub swings: Vec<GameSwing>,
    pub greatest_matchups: Vec<Matchup>,
    /// The weakest game of every season and league, from which `compute` picks each era's.
    pub weakest_per_season: Vec<Matchup>,
    /// Every champion's road; `compute` ranks it both ways.
    pub champion_roads: Vec<ChampionRoad>,
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

/// A run of seasons of one league with the same number of teams; the NBA's run is also cut
/// at the 3-point line.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EraRecord {
    pub league: League,
    pub first_season: SeasonYear,
    pub last_season: SeasonYear,
    pub seasons: usize,
    pub teams: usize,
    pub three_point: bool,
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

/// East against West in one season: mean pre-playoff Elo of each conference.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ConferenceSeason {
    pub season: SeasonYear,
    pub east_teams: usize,
    pub west_teams: usize,
    pub east_mean: f64,
    pub west_mean: f64,
    /// West mean minus East mean: positive when the West was stronger.
    pub gap: f64,
    pub east_best_code: String,
    pub west_best_code: String,
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
    pub by_era: Vec<EraRecord>,
    pub best_end: Vec<Entry>,
    pub worst_end: Vec<Entry>,
    pub biggest_rise: Vec<Entry>,
    pub biggest_fall: Vec<Entry>,
    pub best_champion: Vec<Entry>,
    pub worst_champion: Vec<Entry>,
    pub best_uncrowned: Vec<Entry>,
    /// One row per season with both conferences, in season order.
    pub conference_seasons: Vec<ConferenceSeason>,
    pub divisions: GroupRecords,
    pub swings: Vec<GameSwing>,
    /// Played games with the highest combined pre-game rating, strongest first.
    pub greatest_matchups: Vec<Matchup>,
    /// The weakest game of each era, in era order (NBA/BAA first, then the ABA).
    pub clankers_by_era: Vec<EraClanker>,
    /// The hardest and easiest road to the title of each era, in era order.
    pub roads_by_era: Vec<EraRoads>,
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

/// Eras of one league: consecutive seasons with the same number of teams, and for the NBA a
/// new era at the 3-point line. `sizes` maps each season of the league to its team count.
pub fn eras(
    league: League,
    stats: &[&TeamSeasonStat],
    sizes: &std::collections::BTreeMap<SeasonYear, usize>,
) -> Vec<EraRecord> {
    let mut out: Vec<EraRecord> = Vec::new();
    let mut current: Option<(SeasonYear, SeasonYear, usize)> = None; // (first, last, teams)
    let flush = |era: Option<(SeasonYear, SeasonYear, usize)>, out: &mut Vec<EraRecord>| {
        if let Some((first, last, teams)) = era {
            let members: Vec<&TeamSeasonStat> = stats
                .iter()
                .copied()
                .filter(|s| s.season >= first && s.season <= last)
                .collect();
            if let Some(extremes) = extremes(&members) {
                out.push(EraRecord {
                    league,
                    first_season: first,
                    last_season: last,
                    seasons: (last.end_year() - first.end_year() + 1) as usize,
                    teams,
                    three_point: league == League::Aba || first >= THREE_POINT_SEASON,
                    extremes,
                });
            }
        }
    };
    for (&season, &teams) in sizes {
        let starts_new = match current {
            None => true,
            Some((_, last, n)) => {
                n != teams
                    || last.next() != Some(season)
                    || (league == League::Nba && season == THREE_POINT_SEASON)
            }
        };
        if starts_new {
            flush(current.take(), &mut out);
            current = Some((season, season, teams));
        } else if let Some(c) = current.as_mut() {
            c.1 = season;
        }
    }
    flush(current, &mut out);
    out
}

/// Decade of the season's start year: 1959-60 (end year 1960) belongs to the 1950s.
pub fn decade_of(season: SeasonYear) -> i32 {
    ((season.end_year() - 1) / 10) * 10
}

/// The games floor of every season: [`FULL_SEASON_SHARE`] of the median games played by the
/// teams of that season, rounded up. A team that folded mid-season or an archive row that
/// stops early stays below it; a lockout or a 48-game BAA schedule does not.
fn season_floors(stats: &[TeamSeasonStat]) -> BTreeMap<SeasonYear, i64> {
    let mut games: BTreeMap<SeasonYear, Vec<i64>> = BTreeMap::new();
    for s in stats {
        games.entry(s.season).or_default().push(s.games);
    }
    games
        .into_iter()
        .map(|(season, mut g)| {
            g.sort_unstable();
            let n = g.len();
            let median = if n % 2 == 1 {
                g[n / 2] as f64
            } else {
                (g[n / 2 - 1] + g[n / 2]) as f64 / 2.0
            };
            (season, (median * FULL_SEASON_SHARE).ceil() as i64)
        })
        .collect()
}

/// A finished season for the team: the playoffs were reached (so the regular season is over)
/// and the team played at least the season's games floor.
fn is_full(s: &TeamSeasonStat, floors: &BTreeMap<SeasonYear, i64>) -> bool {
    s.elo_pre_playoffs.is_some() && floors.get(&s.season).is_some_and(|f| s.games >= *f)
}

fn group_records(
    stats: &[TeamSeasonStat],
    floors: &BTreeMap<SeasonYear, i64>,
    key: impl Fn(&TeamSeasonStat) -> Option<(Conference, Option<String>)>,
) -> GroupRecords {
    // Keyed by the conference name because the enum has no ordering of its own.
    type GroupKey = (SeasonYear, &'static str, Option<String>);
    let mut groups: BTreeMap<GroupKey, (Conference, Vec<&TeamSeasonStat>)> = BTreeMap::new();
    for s in stats {
        if !is_full(s, floors) {
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

/// East against West for every season where both conferences have at least two rated teams
/// with full seasons.
fn conference_seasons(
    stats: &[TeamSeasonStat],
    floors: &BTreeMap<SeasonYear, i64>,
) -> Vec<ConferenceSeason> {
    let mut by_season: BTreeMap<SeasonYear, (Vec<&TeamSeasonStat>, Vec<&TeamSeasonStat>)> =
        BTreeMap::new();
    for s in stats {
        if !is_full(s, floors) {
            continue;
        }
        match s.conference {
            Some(Conference::East) => by_season.entry(s.season).or_default().0.push(s),
            Some(Conference::West) => by_season.entry(s.season).or_default().1.push(s),
            None => {}
        }
    }
    let mean = |v: &[&TeamSeasonStat]| {
        v.iter()
            .map(|s| s.elo_pre_playoffs.unwrap_or(0.0))
            .sum::<f64>()
            / v.len() as f64
    };
    let best = |v: &[&TeamSeasonStat]| {
        v.iter()
            .max_by(|a, b| {
                a.elo_pre_playoffs
                    .unwrap_or(0.0)
                    .total_cmp(&b.elo_pre_playoffs.unwrap_or(0.0))
            })
            .map(|s| s.code.clone())
            .unwrap_or_default()
    };
    by_season
        .into_iter()
        .filter(|(_, (e, w))| e.len() >= 2 && w.len() >= 2)
        .map(|(season, (e, w))| {
            let (east_mean, west_mean) = (mean(&e), mean(&w));
            ConferenceSeason {
                season,
                east_teams: e.len(),
                west_teams: w.len(),
                east_mean,
                west_mean,
                gap: west_mean - east_mean,
                east_best_code: best(&e),
                west_best_code: best(&w),
            }
        })
        .collect()
}

/// Build every record table from the team-season statistics and the game-level records the
/// store ranked (swings and matchups). `rosters` lists every (season, team code) with a game
/// on the schedule, played or not; it sets each league's size per season. Seasons missing
/// from it fall back to the teams with played games.
pub fn compute(
    stats: &[TeamSeasonStat],
    games: GameRecords,
    rosters: &[(SeasonYear, String)],
) -> Records {
    let GameRecords {
        swings,
        mut greatest_matchups,
        weakest_per_season,
        champion_roads,
    } = games;
    greatest_matchups.sort_by(|a, b| b.combined_elo().total_cmp(&a.combined_elo()));
    let champion_roads = roads_with_rating(champion_roads);
    let all = || stats.iter();
    let floors = season_floors(stats);

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

    // League sizes: number of teams of that league in that season, from the schedule where
    // it is known (an in-progress season has teams that have not played yet).
    let mut sizes: BTreeMap<(SeasonYear, League), usize> = BTreeMap::new();
    for (season, code) in rosters {
        *sizes.entry((*season, League::of_code(code))).or_default() += 1;
    }
    for s in stats {
        if !rosters.iter().any(|(season, _)| *season == s.season) {
            *sizes
                .entry((s.season, League::of_code(&s.code)))
                .or_default() += 1;
        }
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

    // Eras per league: runs of equal team counts, the NBA's also cut at the 3-point line.
    let mut by_era: Vec<EraRecord> = Vec::new();
    for league in [League::Nba, League::Aba] {
        let members: Vec<&TeamSeasonStat> = stats
            .iter()
            .filter(|s| League::of_code(&s.code) == league)
            .collect();
        let league_sizes: BTreeMap<SeasonYear, usize> = sizes
            .iter()
            .filter(|((_, l), _)| *l == league)
            .map(|((s, _), n)| (*s, *n))
            .collect();
        by_era.extend(eras(league, &members, &league_sizes));
    }
    let clankers_by_era = era_clankers(&by_era, &weakest_per_season);
    let roads_by_era = era_roads(&by_era, &champion_roads);

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
    let full = |s: &TeamSeasonStat| is_full(s, &floors);
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
        by_era,
        best_end: top(all(), end, true, LIST_LEN),
        worst_end: top(all(), end, false, LIST_LEN),
        biggest_rise: top(all(), rise, true, LIST_LEN),
        biggest_fall: top(all(), rise, false, LIST_LEN),
        best_champion: top(champions(), |s| Some(peak(s)), true, LIST_LEN),
        worst_champion: top(champions(), |s| Some(peak(s)), false, LIST_LEN),
        best_uncrowned: top(others(), |s| Some(peak(s)), true, LIST_LEN),
        conference_seasons: conference_seasons(stats, &floors),
        divisions: group_records(stats, &floors, |s| {
            Some((s.conference?, Some(s.division.clone()?)))
        }),
        swings,
        greatest_matchups,
        clankers_by_era,
        roads_by_era,
    }
}

/// The weakest game of each era: the lowest combined rating among the season minima that
/// fall in the era and belong to its league (the league of the home team).
fn era_clankers(eras: &[EraRecord], weakest_per_season: &[Matchup]) -> Vec<EraClanker> {
    eras.iter()
        .filter_map(|e| {
            let game = weakest_per_season
                .iter()
                .filter(|m| {
                    League::of_code(&m.home_code) == e.league
                        && e.first_season <= m.season
                        && m.season <= e.last_season
                })
                .min_by(|a, b| a.combined_elo().total_cmp(&b.combined_elo()))?;
            Some(EraClanker {
                league: e.league,
                first_season: e.first_season,
                last_season: e.last_season,
                teams: e.teams,
                three_point: e.three_point,
                game: game.clone(),
            })
        })
        .collect()
}

/// The champions with games in both phases, each with its road rating filled.
fn roads_with_rating(roads: Vec<ChampionRoad>) -> Vec<ChampionRoad> {
    roads
        .into_iter()
        .filter(|r| r.regular_games > 0 && r.playoff_games > 0)
        .map(|mut r| {
            r.road = r.compute_road();
            r
        })
        .collect()
}

/// The hardest and easiest road of each era among the champions of that era's league.
fn era_roads(eras: &[EraRecord], roads: &[ChampionRoad]) -> Vec<EraRoads> {
    eras.iter()
        .filter_map(|e| {
            let members: Vec<&ChampionRoad> = roads
                .iter()
                .filter(|r| {
                    League::of_code(&r.code) == e.league
                        && e.first_season <= r.season
                        && r.season <= e.last_season
                })
                .collect();
            let hardest = members.iter().max_by(|a, b| a.road.total_cmp(&b.road))?;
            let easiest = members.iter().min_by(|a, b| a.road.total_cmp(&b.road))?;
            Some(EraRoads {
                league: e.league,
                first_season: e.first_season,
                last_season: e.last_season,
                teams: e.teams,
                three_point: e.three_point,
                champions: members.len(),
                hardest: (*hardest).clone(),
                easiest: (*easiest).clone(),
            })
        })
        .collect()
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
    fn eras_split_on_league_size_and_the_three_point_line() {
        // 1978 and 1979: 22 teams; 1980: 22 teams but the 3-point line starts a new era;
        // 1981: 23 teams (expansion) starts another; 1982 continues it.
        let stats: Vec<TeamSeasonStat> =
            [(1978, 22), (1979, 22), (1980, 22), (1981, 23), (1982, 23)]
                .iter()
                .flat_map(|&(season, n)| {
                    (0..n).map(move |i| {
                        stat(
                            season,
                            &format!("T{i:02}"),
                            1500.0 + i as f64,
                            1400.0 - i as f64,
                            PlayoffResult::Missed,
                        )
                    })
                })
                .collect();
        let r = compute(&stats, GameRecords::default(), &[]);
        let nba: Vec<&EraRecord> = r
            .by_era
            .iter()
            .filter(|e| e.league == League::Nba)
            .collect();
        assert_eq!(nba.len(), 3);
        assert_eq!(
            (
                nba[0].first_season,
                nba[0].last_season,
                nba[0].teams,
                nba[0].three_point
            ),
            (SeasonYear::of(1978), SeasonYear::of(1979), 22, false)
        );
        assert_eq!(
            (
                nba[1].first_season,
                nba[1].last_season,
                nba[1].teams,
                nba[1].three_point
            ),
            (SeasonYear::of(1980), SeasonYear::of(1980), 22, true)
        );
        assert_eq!(
            (
                nba[2].first_season,
                nba[2].last_season,
                nba[2].teams,
                nba[2].seasons
            ),
            (SeasonYear::of(1981), SeasonYear::of(1982), 23, 2)
        );
        assert_eq!(nba[2].extremes.best.value, 1522.0);
        assert_eq!(nba[2].extremes.worst.value, 1400.0 - 22.0);
    }

    #[test]
    fn league_size_comes_from_the_schedule_not_from_games_played() {
        // Two days into 2026-27 only 28 teams have played, but the schedule lists 30; the
        // season must not become a 28-team era of its own.
        let mut stats: Vec<TeamSeasonStat> = (0..30)
            .map(|i| {
                stat(
                    2026,
                    &format!("T{i:02}"),
                    1500.0 + i as f64,
                    1400.0,
                    PlayoffResult::Missed,
                )
            })
            .collect();
        stats.extend((0..28).map(|i| {
            let mut s = stat(
                2027,
                &format!("T{i:02}"),
                1500.0,
                1450.0,
                PlayoffResult::Missed,
            );
            s.games = 1;
            s
        }));
        let rosters: Vec<(SeasonYear, String)> = (0..30)
            .flat_map(|i| {
                [
                    (SeasonYear::of(2026), format!("T{i:02}")),
                    (SeasonYear::of(2027), format!("T{i:02}")),
                ]
            })
            .collect();
        let r = compute(&stats, GameRecords::default(), &rosters);
        assert_eq!(r.by_era.len(), 1);
        assert_eq!((r.by_era[0].teams, r.by_era[0].seasons), (30, 2));
        assert_eq!(r.by_league_size.len(), 1);
        assert_eq!(r.by_league_size[0].teams, 30);
        // Without rosters the played games decide, and the seasons split.
        assert_eq!(compute(&stats, GameRecords::default(), &[]).by_era.len(), 2);
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
        let r = compute(&stats, GameRecords::default(), &[]);
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
        // Conferences: no season here has two rated teams on both sides, so no comparison row.
        assert!(r.conference_seasons.is_empty());
    }

    #[test]
    fn clankers_take_the_weakest_game_of_each_era_within_its_league() {
        let game = |year: i32, home: &str, away: &str, h: f64, a: f64| Matchup {
            season: SeasonYear::of(year),
            game_date: d(&format!("{year}-01-15")),
            home_code: home.to_string(),
            home_name: home.to_string(),
            away_code: away.to_string(),
            away_name: away.to_string(),
            home_elo_pre: h,
            away_elo_pre: a,
            home_score: 100,
            away_score: 90,
            playoff: false,
        };
        let era = |league, first: i32, last: i32| EraRecord {
            league,
            first_season: SeasonYear::of(first),
            last_season: SeasonYear::of(last),
            seasons: (last - first + 1) as usize,
            teams: 10,
            three_point: false,
            extremes: Extremes {
                best: entry(
                    &stat(first, "X", 1500.0, 1400.0, PlayoffResult::Missed),
                    1500.0,
                    None,
                ),
                worst: entry(
                    &stat(first, "Y", 1500.0, 1400.0, PlayoffResult::Missed),
                    1400.0,
                    None,
                ),
            },
        };
        let eras = vec![
            era(League::Nba, 1970, 1971),
            era(League::Nba, 1972, 1973),
            era(League::Aba, 1970, 1973),
        ];
        let per_season = vec![
            game(1970, "BOS", "NYK", 1400.0, 1450.0), // NBA 1970: 2850
            game(1971, "BUF", "CLE", 1200.0, 1150.0), // NBA 1971: 2350 -> era 1 clanker
            game(1972, "POR", "CLE", 1300.0, 1250.0), // NBA 1972: 2550 -> era 2 clanker
            game(1970, "KEN", "INA", 1100.0, 1150.0), // ABA 1970: 2250 -> ABA clanker
            game(1973, "SDA", "MMT", 1200.0, 1300.0), // ABA 1973: 2500
        ];
        let c = era_clankers(&eras, &per_season);
        let picks: Vec<(League, i32, &str)> = c
            .iter()
            .map(|e| {
                (
                    e.league,
                    e.game.season.end_year(),
                    e.game.home_code.as_str(),
                )
            })
            .collect();
        assert_eq!(
            picks,
            vec![
                (League::Nba, 1971, "BUF"),
                (League::Nba, 1972, "POR"),
                (League::Aba, 1970, "KEN")
            ]
        );
    }

    #[test]
    fn roads_by_era_pick_the_hardest_and_easiest_champion_of_each_era() {
        let road =
            |year: i32, code: &str, regular: f64, playoff: f64, playoff_games: i64| ChampionRoad {
                season: SeasonYear::of(year),
                code: code.to_string(),
                name: code.to_string(),
                regular_opponent_elo: regular,
                regular_games: 82,
                playoff_opponent_elo: playoff,
                playoff_games,
                toughest_opponent_code: "OPP".to_string(),
                toughest_opponent_elo: playoff + 50.0,
                road: 0.0,
            };
        let roads = vec![
            road(2016, "CLE", 1500.0, 1700.0, 21), // (1500*82 + 1700*21) / 103 = 1540.8
            road(2017, "GSW", 1510.0, 1650.0, 17), // (1510*82 + 1650*17) / 99 = 1534.0
            road(1999, "SAS", 1490.0, 1600.0, 17), // (1490*82 + 1600*17) / 99 = 1508.9
            road(2025, "UNF", 1600.0, 0.0, 0),     // no playoffs yet: left out
        ];
        let rated = roads_with_rating(roads);
        assert_eq!(rated.len(), 3, "the champion without playoffs is left out");
        assert!((rated[0].road - (1500.0 * 82.0 + 1700.0 * 21.0) / 103.0).abs() < 1e-9);
        let era = |first: i32, last: i32| EraRecord {
            league: League::Nba,
            first_season: SeasonYear::of(first),
            last_season: SeasonYear::of(last),
            seasons: (last - first + 1) as usize,
            teams: 30,
            three_point: true,
            extremes: Extremes {
                best: entry(
                    &stat(first, "X", 1500.0, 1400.0, PlayoffResult::Missed),
                    1500.0,
                    None,
                ),
                worst: entry(
                    &stat(first, "Y", 1500.0, 1400.0, PlayoffResult::Missed),
                    1400.0,
                    None,
                ),
            },
        };
        let by_era = era_roads(&[era(1996, 2004), era(2005, 2026)], &rated);
        assert_eq!(by_era.len(), 2);
        assert_eq!(
            (by_era[0].champions, by_era[0].hardest.code.as_str()),
            (1, "SAS")
        );
        assert_eq!(by_era[0].easiest.code, "SAS");
        assert_eq!(
            (
                by_era[1].champions,
                by_era[1].hardest.code.as_str(),
                by_era[1].easiest.code.as_str()
            ),
            (2, "CLE", "GSW")
        );
        // An era without a champion in the list yields no row.
        assert!(era_roads(&[era(1947, 1950)], &rated).is_empty());
    }

    #[test]
    fn the_games_floor_follows_each_season_not_a_fixed_count() {
        // A 50-game lockout season is full; a team that folded after 14 games of a 72-game
        // season is not; a season that has not reached its playoffs never counts.
        let mut stats = vec![
            stat(1999, "WSA", 1700.0, 1600.0, PlayoffResult::Champion),
            stat(1999, "WUT", 1690.0, 1600.0, PlayoffResult::SecondRound),
            stat(1999, "ENY", 1600.0, 1500.0, PlayoffResult::Finals),
            stat(1999, "EMI", 1620.0, 1500.0, PlayoffResult::FirstRound),
            stat(1955, "EBA", 1400.0, 1300.0, PlayoffResult::Missed),
            stat(1955, "ESY", 1600.0, 1500.0, PlayoffResult::Champion),
            stat(1955, "EBO", 1550.0, 1450.0, PlayoffResult::FirstRound),
            stat(1955, "WFW", 1580.0, 1450.0, PlayoffResult::Finals),
            stat(1955, "WMN", 1500.0, 1400.0, PlayoffResult::FirstRound),
            stat(2027, "WLA", 1550.0, 1500.0, PlayoffResult::Missed),
            stat(2027, "EBO", 1550.0, 1500.0, PlayoffResult::Missed),
        ];
        for s in stats.iter_mut().take(4) {
            s.games = if s.code == "WSA" { 67 } else { 50 };
        }
        stats[4].games = 14; // Baltimore folded
        for s in stats.iter_mut().skip(5).take(4) {
            s.games = 75;
        }
        for s in stats.iter_mut().skip(9) {
            s.games = 5;
            s.elo_pre_playoffs = None;
        }
        let floors = season_floors(&stats);
        assert_eq!(floors[&SeasonYear::of(1999)], 40); // median 50 -> 0.8 * 50
        assert_eq!(floors[&SeasonYear::of(1955)], 60); // median 75
        assert_eq!(floors[&SeasonYear::of(2027)], 4);
        let r = compute(&stats, GameRecords::default(), &[]);
        let seasons: Vec<i32> = r
            .conference_seasons
            .iter()
            .map(|c| c.season.end_year())
            .collect();
        assert_eq!(seasons, vec![1955, 1999]);
        let c1955 = &r.conference_seasons[0];
        assert_eq!((c1955.east_teams, c1955.west_teams), (2, 2));
        assert!(r.best_end.iter().all(|e| e.season.end_year() != 2027));
        assert!(r.best_end.iter().any(|e| e.season.end_year() == 1999));
    }

    #[test]
    fn conference_and_division_means_need_two_teams_and_full_seasons() {
        let mut stats = vec![
            stat(2017, "WGS", 1865.0, 1700.0, PlayoffResult::Champion),
            stat(2017, "WLA", 1500.0, 1400.0, PlayoffResult::Missed),
            stat(2017, "WHO", 1700.0, 1500.0, PlayoffResult::SecondRound),
            stat(2017, "EBO", 1600.0, 1500.0, PlayoffResult::ConfFinals),
            stat(2017, "ECL", 1750.0, 1600.0, PlayoffResult::Finals),
        ];
        stats[1].games = 10; // an unfinished season does not count
        let r = compute(&stats, GameRecords::default(), &[]);
        assert_eq!(r.conference_seasons.len(), 1);
        let c = &r.conference_seasons[0];
        assert_eq!((c.east_teams, c.west_teams), (2, 2));
        assert!((c.east_mean - (1580.0 + 1730.0) / 2.0).abs() < 1e-9);
        assert!((c.west_mean - (1845.0 + 1680.0) / 2.0).abs() < 1e-9);
        assert!((c.gap - (c.west_mean - c.east_mean)).abs() < 1e-9);
        assert_eq!(
            (c.east_best_code.as_str(), c.west_best_code.as_str()),
            ("ECL", "WGS")
        );
        // Divisions keep the best/worst lists: the Pacific (West) outrates the Atlantic here.
        assert_eq!(r.divisions.best[0].division.as_deref(), Some("Pacific"));
        assert_eq!(r.divisions.worst[0].division.as_deref(), Some("Atlantic"));
    }
}
