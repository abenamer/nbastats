//! Collective bargaining periods of the NBA and the records grouped by them.
//!
//! A bargaining period starts with the season in which a new agreement between the league
//! and the players' association took effect (or, before the first agreement with a salary
//! cap, with the settlement that changed the rules of player movement). The table is
//! curated from the public record; each period carries a one-line summary and the changes
//! it brought, shown on the Records page. The ABA had no agreement of this kind, so only
//! NBA/BAA seasons belong to a period.

use std::collections::BTreeMap;

use serde::Serialize;

use crate::domain::ids::SeasonYear;
use crate::domain::models::PlayoffResult;
use crate::domain::records::{
    change, low, peak, Change, ChangeNote, Entry, League, TeamSeasonStat,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct CbaPeriod {
    pub first_season: SeasonYear,
    pub name: &'static str,
    pub summary: &'static str,
    pub changes: &'static [Change],
}

/// The bargaining periods, in season order. Dates are the first season an agreement
/// applied to.
pub const CBA_PERIODS: &[CbaPeriod] = &[
    CbaPeriod {
        first_season: SeasonYear::of(1947),
        name: "Pre-cap",
        summary: "Reserve clause, no salary cap; pay set by each negotiation, pushed up by the ABA from 1967-68.",
        changes: &[
            change(1947, "The reserve clause binds a player to his team for as long as the team wants him."),
            change(1955, "The players' association forms around Bob Cousy; the owners recognise it in 1957."),
            change(1964, "A threatened All-Star Game boycott wins the first pension plan."),
            change(1968, "The ABA opens; competing offers raise salaries in both leagues."),
            change(1971, "Oscar Robertson and the union sue against the reserve clause and the planned merger."),
        ],
    },
    CbaPeriod {
        first_season: SeasonYear::of(1977),
        name: "Robertson settlement",
        summary: "The 1976 settlement ends the reserve clause: free agency with compensation, then the right of first refusal from 1981-82.",
        changes: &[
            change(1977, "Robertson settlement: the reserve clause ends; a team that loses a free agent gets compensation set by the commissioner."),
            change(1977, "The ABA merger adds Denver, Indiana, the New York Nets and San Antonio."),
            change(1982, "Compensation gives way to the right of first refusal: a team can match an offer to its free agent."),
            change(1984, "The 1983 agreement sets the first salary cap, a drug program and a 53 percent revenue share, to start in 1984-85."),
        ],
    },
    CbaPeriod {
        first_season: SeasonYear::of(1985),
        name: "First salary cap",
        summary: "A cap of about 3.6 million per team, 53 percent of revenue, with the Bird exception to re-sign a team's own players.",
        changes: &[
            change(1985, "Salary cap of about 3.6 million per team, 53 percent of defined revenue."),
            change(1985, "Bird exception: a team may exceed the cap to re-sign its own free agents."),
            change(1985, "Minimum team payroll; an anti-drug agreement with lifetime bans."),
        ],
    },
    CbaPeriod {
        first_season: SeasonYear::of(1989),
        name: "1988 agreement",
        summary: "Unrestricted free agency phased in, the draft cut to two rounds, expansion to 27 teams; rookie pay still unregulated.",
        changes: &[
            change(1989, "Unrestricted free agency for veterans, phased in by years of service."),
            change(1990, "The draft is shortened to two rounds (from the 1989 draft)."),
            change(1989, "Expansion: Miami and Charlotte in 1988-89, Minnesota and Orlando in 1989-90."),
            change(1995, "Rookie contracts stay unregulated; record first contracts lead to the 1995 scale."),
        ],
    },
    CbaPeriod {
        first_season: SeasonYear::of(1996),
        name: "1995 agreement",
        summary: "Rookie wage scale, a minimum team salary, Toronto and Vancouver; no maximum contract yet.",
        changes: &[
            change(1996, "Rookie scale: first-round picks sign fixed three-year contracts."),
            change(1996, "Expansion to 29 teams: Toronto and Vancouver."),
            change(1996, "A union decertification vote fails; the agreement is ratified after a short lockout."),
            change(1998, "Without an individual maximum, Kevin Garnett's 126-million extension leads the owners to reopen the agreement and lock the players out."),
        ],
    },
    CbaPeriod {
        first_season: SeasonYear::of(1999),
        name: "1999 agreement",
        summary: "After the lockout: maximum salaries by years of service, a luxury tax, escrow and the mid-level exception.",
        changes: &[
            change(1999, "The lockout cuts 1998-99 to 50 games."),
            change(1999, "Maximum individual salary by years of service: 25, 30 or 35 percent of the cap."),
            change(1999, "A luxury tax above a line near 61 percent of revenue, first levied in 2002-03; escrow withholds 10 percent of salaries."),
            change(1999, "Mid-level and minimum exceptions; contracts capped at seven years; the rookie scale extended to four years with restricted free agency."),
        ],
    },
    CbaPeriod {
        first_season: SeasonYear::of(2006),
        name: "2005 agreement",
        summary: "Draft age 19, shorter contracts and smaller raises, a yearly dollar-for-dollar tax, a one-time amnesty.",
        changes: &[
            change(2006, "Draft eligibility: 19 years old and one year out of high school."),
            change(2006, "Maximum contract length cut to six years (five with another team); annual raises reduced."),
            change(2006, "The luxury tax applies every season at one dollar per dollar over the line; players get 57 percent of revenue."),
            change(2006, "A one-time amnesty waiver removes a contract from the tax; first and second-year players can be assigned to the D-League."),
        ],
    },
    CbaPeriod {
        first_season: SeasonYear::of(2012),
        name: "2011 agreement",
        summary: "After the lockout: a revenue split near 50-50, a progressive luxury tax with a repeater rate, the tax apron.",
        changes: &[
            change(2012, "The lockout cuts 2011-12 to 66 games; the players' share drops from 57 to about 50 percent of revenue."),
            change(2014, "A progressive luxury tax from 1.50 to 3.75 dollars per dollar, with a repeater rate from 2014-15."),
            change(2012, "The tax apron: teams above it lose the full mid-level exception and sign-and-trade acquisitions."),
            change(2012, "Shorter contracts (five years to re-sign, four otherwise), smaller raises, the stretch provision and a designated-player rule."),
        ],
    },
    CbaPeriod {
        first_season: SeasonYear::of(2018),
        name: "2017 agreement",
        summary: "Supermax (designated veteran) extensions, two-way contracts, a longer calendar with fewer back-to-backs.",
        changes: &[
            change(2018, "Designated veteran extension: 35 percent of the cap for a qualifying player with his own team."),
            change(2018, "Two-way contracts and 17-man rosters; the G League becomes the development path."),
            change(2018, "The season starts a week earlier to cut back-to-backs; the preseason is shortened."),
            change(2021, "Pandemic amendments: a 72-game 2020-21 season; the play-in tournament becomes permanent in 2021-22."),
        ],
    },
    CbaPeriod {
        first_season: SeasonYear::of(2024),
        name: "2023 agreement",
        summary: "A second apron with hard limits on trades and exceptions, a 65-game rule for awards, the NBA Cup.",
        changes: &[
            change(2024, "Second apron: teams above it cannot aggregate salaries in trades, use the taxpayer mid-level exception or send cash, and a future first-round pick is frozen."),
            change(2024, "The in-season tournament (the NBA Cup) with a neutral-site final."),
            change(2024, "A 65-game minimum to qualify for the major awards and the All-NBA teams."),
            change(2024, "A third two-way slot; cap growth limited to 10 percent a year under the new media agreement."),
        ],
    },
];

/// The bargaining period an NBA/BAA season belongs to: the latest period that starts on or
/// before it.
pub fn cba_of(season: SeasonYear) -> &'static CbaPeriod {
    CBA_PERIODS
        .iter()
        .rev()
        .find(|p| p.first_season <= season)
        .unwrap_or(&CBA_PERIODS[0])
}

/// A franchise's titles within a group.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TitleCount {
    pub franchise_key: String,
    /// The code and name of the franchise's latest title in the group.
    pub code: String,
    pub name: String,
    pub titles: usize,
}

/// The records of a group of team-seasons: a bargaining period, or any group the era
/// builder makes.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GroupStats {
    pub best: Entry,
    pub weakest: Entry,
    /// Titles recorded in the group and the franchises that won them, most titles first.
    pub titles: usize,
    pub champions: Vec<TitleCount>,
    pub distinct_champions: usize,
    /// Herfindahl-Hirschman index of the titles: the sum of each champion's squared share.
    /// 1.0 when one franchise won every title; `None` without a title.
    pub title_hhi: Option<f64>,
    /// The mean season-end rating across the league, averaged over the group's seasons: the
    /// level the ratings sat at, which shows whether they drift over time.
    pub mean_elo: Option<f64>,
    /// The standard deviation of season-end ratings across the league, averaged over the
    /// group's seasons: a measure of how unequal the league was.
    pub elo_spread: Option<f64>,
    /// The champion's pre-playoff rating above the league mean in standard deviations,
    /// averaged over the group's seasons with a champion (see `domain::parity`): how far
    /// the winner stood above its league.
    pub champion_z: Option<f64>,
}

/// A bargaining period with the records of its seasons in the data.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CbaSummary {
    pub name: String,
    pub summary: String,
    /// The period's first season by the agreement, and the seasons present in the data.
    pub period_start: SeasonYear,
    pub first_season: SeasonYear,
    pub last_season: SeasonYear,
    pub seasons: usize,
    #[serde(flatten)]
    pub stats: GroupStats,
    pub changes: Vec<ChangeNote>,
}

/// The Herfindahl-Hirschman index of a title distribution.
pub fn title_hhi(counts: &[usize]) -> Option<f64> {
    let total: usize = counts.iter().sum();
    if total == 0 {
        return None;
    }
    Some(
        counts
            .iter()
            .map(|&n| (n as f64 / total as f64).powi(2))
            .sum(),
    )
}

/// The finite season-end ratings of `stats`, by season.
fn ratings_by_season(stats: &[&TeamSeasonStat]) -> BTreeMap<SeasonYear, Vec<f64>> {
    let mut by_season: BTreeMap<SeasonYear, Vec<f64>> = BTreeMap::new();
    for s in stats {
        if let Some(e) = s.elo_end.filter(|e| e.is_finite()) {
            by_season.entry(s.season).or_default().push(e);
        }
    }
    by_season
}

fn average(values: &[f64]) -> Option<f64> {
    if values.is_empty() {
        None
    } else {
        Some(values.iter().sum::<f64>() / values.len() as f64)
    }
}

/// The population standard deviation of the season-end ratings of `stats`, per season,
/// averaged over the seasons with at least two rated teams.
pub fn elo_spread(stats: &[&TeamSeasonStat]) -> Option<f64> {
    let spreads: Vec<f64> = ratings_by_season(stats)
        .values()
        .filter(|v| v.len() >= 2)
        .map(|v| {
            let mean = v.iter().sum::<f64>() / v.len() as f64;
            (v.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / v.len() as f64).sqrt()
        })
        .collect();
    average(&spreads)
}

/// The mean season-end rating of `stats`, per season, averaged over the seasons, so every
/// season weighs the same whatever its team count.
pub fn mean_elo(stats: &[&TeamSeasonStat]) -> Option<f64> {
    let means: Vec<f64> = ratings_by_season(stats)
        .values()
        .filter_map(|v| average(v))
        .collect();
    average(&means)
}

/// The records of a group of team-seasons; `None` when the group is empty. Titles are
/// grouped by franchise, so a relocated champion keeps its count.
pub fn group_stats(members: &[&TeamSeasonStat]) -> Option<GroupStats> {
    let best = members
        .iter()
        .max_by(|a, b| a.peak.total_cmp(&b.peak).then(b.season.cmp(&a.season)))?;
    let weakest = members
        .iter()
        .min_by(|a, b| a.low.total_cmp(&b.low).then(b.season.cmp(&a.season)))?;
    let mut by_franchise: BTreeMap<&str, TitleCount> = BTreeMap::new();
    for s in members
        .iter()
        .filter(|s| s.playoff_result == PlayoffResult::Champion)
    {
        let t = by_franchise
            .entry(s.franchise_key.as_str())
            .or_insert_with(|| TitleCount {
                franchise_key: s.franchise_key.clone(),
                code: s.code.clone(),
                name: s.name.clone(),
                titles: 0,
            });
        t.titles += 1;
        t.code = s.code.clone();
        t.name = s.name.clone();
    }
    let mut champions: Vec<TitleCount> = by_franchise.into_values().collect();
    champions.sort_by(|a, b| b.titles.cmp(&a.titles).then(a.code.cmp(&b.code)));
    let counts: Vec<usize> = champions.iter().map(|c| c.titles).collect();
    Some(GroupStats {
        best: peak(best),
        weakest: low(weakest),
        titles: counts.iter().sum(),
        distinct_champions: champions.len(),
        title_hhi: title_hhi(&counts),
        champions,
        mean_elo: mean_elo(members),
        elo_spread: elo_spread(members),
        champion_z: super::parity::summary(&super::parity::profile(members))
            .and_then(|s| s.champion_z),
    })
}

/// The bargaining periods present in the data, each with its records, in season order.
pub fn summaries(stats: &[TeamSeasonStat]) -> Vec<CbaSummary> {
    CBA_PERIODS
        .iter()
        .filter_map(|p| {
            let mine: Vec<&TeamSeasonStat> = stats
                .iter()
                .filter(|s| League::of_code(&s.code) == League::Nba && cba_of(s.season) == p)
                .collect();
            let mut seasons: Vec<SeasonYear> = mine.iter().map(|s| s.season).collect();
            seasons.sort_unstable();
            seasons.dedup();
            Some(CbaSummary {
                name: p.name.to_string(),
                summary: p.summary.to_string(),
                period_start: p.first_season,
                first_season: *seasons.first()?,
                last_season: *seasons.last()?,
                seasons: seasons.len(),
                stats: group_stats(&mine)?,
                changes: p.changes.iter().map(ChangeNote::from).collect(),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::models::GameType;
    use chrono::NaiveDate;

    fn stat(
        season: i32,
        code: &str,
        key: &str,
        peak: f64,
        end: f64,
        champion: bool,
    ) -> TeamSeasonStat {
        TeamSeasonStat {
            season: SeasonYear::of(season),
            code: code.to_string(),
            name: format!("Team {code}"),
            franchise_key: key.to_string(),
            games: 82,
            peak,
            peak_date: NaiveDate::from_ymd_opt(season, 5, 1).unwrap(),
            peak_game_type: GameType::Regular,
            low: peak - 200.0,
            low_date: NaiveDate::from_ymd_opt(season - 1, 11, 1).unwrap(),
            low_game_type: GameType::Regular,
            wins: 50,
            losses: 32,
            playoff_wins: 0,
            playoff_losses: 0,
            elo_start: Some(1500.0),
            elo_pre_playoffs: Some(end),
            elo_end: Some(end),
            playoff_result: if champion {
                PlayoffResult::Champion
            } else {
                PlayoffResult::FirstRound
            },
            conference: None,
            division: None,
        }
    }

    #[test]
    fn group_statistics_carry_the_champion_z_of_the_parity_profile() {
        // Eight teams rated 1400 to 1540 before the playoffs (mean 1470); the best wins.
        let stats: Vec<TeamSeasonStat> = (0..8)
            .map(|i| {
                let mut s = TeamSeasonStat {
                    season: SeasonYear::of(2015),
                    code: format!("T{i}"),
                    name: format!("T{i}"),
                    franchise_key: format!("T{i}"),
                    games: 82,
                    peak: 1500.0,
                    peak_date: chrono::NaiveDate::from_ymd_opt(2015, 4, 1).unwrap(),
                    peak_game_type: crate::domain::models::GameType::Regular,
                    low: 1400.0,
                    low_date: chrono::NaiveDate::from_ymd_opt(2014, 11, 1).unwrap(),
                    low_game_type: crate::domain::models::GameType::Regular,
                    wins: 41,
                    losses: 41,
                    playoff_wins: 0,
                    playoff_losses: 0,
                    elo_start: Some(1500.0),
                    elo_pre_playoffs: Some(1400.0 + 20.0 * i as f64),
                    elo_end: Some(1400.0 + 20.0 * i as f64),
                    playoff_result: PlayoffResult::Missed,
                    conference: None,
                    division: None,
                };
                if i >= 4 {
                    s.playoff_result = PlayoffResult::FirstRound;
                }
                if i == 7 {
                    s.playoff_result = PlayoffResult::Champion;
                }
                s
            })
            .collect();
        let members: Vec<&TeamSeasonStat> = stats.iter().collect();
        let st = group_stats(&members).unwrap();
        let expected = super::super::parity::profile(&members)[0]
            .champion
            .as_ref()
            .unwrap()
            .z;
        assert!((st.champion_z.unwrap() - expected).abs() < 1e-12);
        assert!(expected > 1.0);
        // A group without a champion has no champion z.
        let no_champion: Vec<&TeamSeasonStat> = members[..7].to_vec();
        assert_eq!(group_stats(&no_champion).unwrap().champion_z, None);
    }

    #[test]
    fn periods_resolve_by_season_and_start_with_the_agreement() {
        assert_eq!(cba_of(SeasonYear::of(1947)).name, "Pre-cap");
        assert_eq!(cba_of(SeasonYear::of(1984)).name, "Robertson settlement");
        assert_eq!(cba_of(SeasonYear::of(1985)).name, "First salary cap");
        assert_eq!(cba_of(SeasonYear::of(1999)).name, "1999 agreement");
        assert_eq!(cba_of(SeasonYear::of(2011)).name, "2005 agreement");
        assert_eq!(cba_of(SeasonYear::of(2012)).name, "2011 agreement");
        assert_eq!(cba_of(SeasonYear::of(2100)).name, "2023 agreement");
        let starts: Vec<i32> = CBA_PERIODS
            .iter()
            .map(|p| p.first_season.end_year())
            .collect();
        assert!(starts.windows(2).all(|w| w[0] < w[1]), "periods in order");
        assert!(CBA_PERIODS.iter().all(|p| !p.changes.is_empty()));
    }

    #[test]
    fn hhi_and_spread_follow_their_definitions() {
        assert_eq!(title_hhi(&[]), None);
        assert_eq!(title_hhi(&[6]), Some(1.0));
        let six = title_hhi(&[1, 1, 1, 1, 1, 1]).unwrap();
        assert!((six - 1.0 / 6.0).abs() < 1e-12);
        let mixed = title_hhi(&[3, 2, 1]).unwrap();
        assert!((mixed - (9.0 + 4.0 + 1.0) / 36.0).abs() < 1e-12);
        let a = stat(2000, "A", "A", 1700.0, 1600.0, true);
        let b = stat(2000, "B", "B", 1500.0, 1400.0, false);
        let lone = stat(2001, "A", "A", 1700.0, 1600.0, false);
        assert_eq!(elo_spread(&[&a, &b]), Some(100.0));
        // A season with one rated team does not count.
        assert_eq!(elo_spread(&[&a, &b, &lone]), Some(100.0));
        assert_eq!(elo_spread(&[&lone]), None);
        // A rating that is not a number is ignored, not propagated.
        let mut nan = stat(2000, "C", "C", 1500.0, 1400.0, false);
        nan.elo_end = Some(f64::NAN);
        assert_eq!(elo_spread(&[&a, &b, &nan]), Some(100.0));
        // The mean weighs every season the same: 1500 in 2000, 1600 in 2001.
        assert_eq!(mean_elo(&[&a, &b]), Some(1500.0));
        assert_eq!(mean_elo(&[&a, &b, &lone]), Some(1550.0));
        assert_eq!(mean_elo(&[&a, &b, &nan]), Some(1500.0));
        assert_eq!(mean_elo(&[&nan]), None);
        assert_eq!(mean_elo(&[]), None);
    }

    #[test]
    fn summaries_group_titles_by_franchise_within_the_period() {
        let stats = vec![
            // 1999 agreement: 1998-99 to 2004-05; the Lakers relocate codes within a franchise.
            stat(1999, "SAS", "SAS", 1750.0, 1700.0, true),
            stat(1999, "LAL", "LAL", 1650.0, 1600.0, false),
            stat(2000, "LAL", "LAL", 1800.0, 1760.0, true),
            stat(2000, "SAS", "SAS", 1700.0, 1650.0, false),
            stat(2001, "LAL", "LAL", 1820.0, 1790.0, true),
            stat(2001, "SAS", "SAS", 1710.0, 1660.0, false),
            // 2005 agreement: 2005-06 on.
            stat(2006, "MIA", "MIA", 1680.0, 1650.0, true),
            stat(2006, "DAL", "DAL", 1700.0, 1660.0, false),
            // An ABA season is not in any period.
            stat(1972, "INA", "INA", 1700.0, 1650.0, true),
        ];
        let out = summaries(&stats);
        assert_eq!(out.len(), 2);
        let p = &out[0];
        assert_eq!(
            (
                p.name.as_str(),
                p.seasons,
                p.stats.titles,
                p.stats.distinct_champions
            ),
            ("1999 agreement", 3, 3, 2)
        );
        assert_eq!(p.first_season, SeasonYear::of(1999));
        assert_eq!(p.last_season, SeasonYear::of(2001));
        assert_eq!(p.stats.champions[0].code, "LAL");
        assert_eq!(p.stats.champions[0].titles, 2);
        assert_eq!(p.stats.best.code, "LAL");
        assert_eq!(p.stats.best.season, SeasonYear::of(2001));
        assert!((p.stats.title_hhi.unwrap() - (4.0 + 1.0) / 9.0).abs() < 1e-12);
        assert!((p.stats.elo_spread.unwrap() - (50.0 + 55.0 + 65.0) / 3.0).abs() < 1e-9);
        assert!(p.stats.mean_elo.is_some_and(f64::is_finite));
        assert_eq!(p.changes.len(), 4);
        assert_eq!(
            (
                out[1].name.as_str(),
                out[1].stats.titles,
                out[1].stats.title_hhi
            ),
            ("2005 agreement", 1, Some(1.0))
        );
    }
}
