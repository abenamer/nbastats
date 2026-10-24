//! The parity profile: season by season, how far the champion, the two finalists, the final
//! four and the playoff field sat above the league, in Elo points and in standard
//! deviations of the league's pre-playoff ratings.
//!
//! The z-score is the consistent reading across seasons: a gap of 100 Elo points means more
//! in a tight league than in a spread-out one, and the spread of the ratings changes with
//! the league's size and era. A champion at z = 2.5 stood far above its league; one at z
//! near 1 won from the pack. The mean z of a group of seasons is its parity figure.

use std::collections::BTreeMap;

use serde::Serialize;

use crate::domain::ids::SeasonYear;
use crate::domain::models::PlayoffResult;
use crate::domain::records::{League, TeamSeasonStat};

/// The fewest rated teams for a season to have a profile: a spread over a handful of teams
/// says little.
pub const MIN_TEAMS: usize = 8;
/// The fewest teams for a playoff field: a season whose only labelled team is its champion
/// has no field to measure.
pub const MIN_FIELD: usize = 4;

/// A group of teams measured against the league of its season.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Tier {
    pub teams: usize,
    /// Mean pre-playoff rating of the group.
    pub mean: f64,
    /// The group's mean minus the league mean, in Elo points.
    pub gap: f64,
    /// The gap in standard deviations of the league's pre-playoff ratings.
    pub z: f64,
    /// The group's mean minus the mean of the other teams, in Elo points; 0 when the group
    /// is the whole league.
    pub gap_to_rest: f64,
}

/// One season's profile.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SeasonParity {
    pub season: SeasonYear,
    pub league: League,
    /// Rated teams with a full season.
    pub teams: usize,
    pub league_mean: f64,
    /// Population standard deviation of the pre-playoff ratings.
    pub league_sd: f64,
    pub champion_code: Option<String>,
    /// The champion's place among the league's pre-playoff ratings, 1 for the best.
    pub champion_rank: Option<usize>,
    /// `None` when the archive names no champion among the rated teams.
    pub champion: Option<Tier>,
    /// The two teams that reached the Finals; `None` unless both are rated.
    pub finalists: Option<Tier>,
    /// The four teams that reached the conference finals; `None` unless all four are rated.
    pub final_four: Option<Tier>,
    /// The teams that reached the first round; `None` with fewer than [`MIN_FIELD`].
    pub playoff_field: Option<Tier>,
}

/// The parity figures of a group of seasons: each a mean over the seasons that have the tier.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ParitySummary {
    pub seasons: usize,
    /// Seasons with a champion tier, the denominator of `champion_was_best`.
    pub seasons_with_champion: usize,
    pub mean_league_sd: f64,
    pub champion_z: Option<f64>,
    pub finalists_z: Option<f64>,
    pub final_four_z: Option<f64>,
    pub playoff_field_z: Option<f64>,
    pub champion_gap: Option<f64>,
    /// Seasons whose champion had the league's best pre-playoff rating.
    pub champion_was_best: usize,
    pub mean_champion_rank: Option<f64>,
}

fn mean(values: &[f64]) -> Option<f64> {
    (!values.is_empty()).then(|| values.iter().sum::<f64>() / values.len() as f64)
}

fn tier(members: &[f64], league_mean: f64, league_sd: f64, league_sum: f64, teams: usize) -> Tier {
    let m = members.iter().sum::<f64>() / members.len() as f64;
    let rest = teams - members.len();
    let rest_mean = if rest == 0 {
        m
    } else {
        (league_sum - members.iter().sum::<f64>()) / rest as f64
    };
    Tier {
        teams: members.len(),
        mean: m,
        gap: m - league_mean,
        z: if league_sd > 0.0 {
            (m - league_mean) / league_sd
        } else {
            0.0
        },
        gap_to_rest: m - rest_mean,
    }
}

/// The profile of every season with at least [`MIN_TEAMS`] rated full-season teams and some
/// postseason information (a champion or a team labelled as a playoff team), per league, in
/// season order (NBA/BAA before the ABA within a season). A season in progress has ratings
/// but no postseason yet, so it has no profile. A team-season counts as full when it played
/// at least the games floor of its season and league (`records::games_floor`, the rule the
/// conference tables use), so a team that folded in the autumn does not widen the spread.
pub fn profile(stats: &[&TeamSeasonStat]) -> Vec<SeasonParity> {
    let mut by_season: BTreeMap<(SeasonYear, League), Vec<&TeamSeasonStat>> = BTreeMap::new();
    for s in stats {
        by_season
            .entry((s.season, League::of_code(&s.code)))
            .or_default()
            .push(s);
    }
    by_season
        .into_iter()
        .filter_map(|((season, league), all)| {
            if !all
                .iter()
                .any(|s| s.playoff_result >= PlayoffResult::FirstRound)
            {
                return None;
            }
            let floor =
                super::records::games_floor(&all.iter().map(|s| s.games).collect::<Vec<_>>());
            let rated: Vec<(&TeamSeasonStat, f64)> = all
                .iter()
                .filter(|s| s.games >= floor)
                .filter_map(|s| {
                    s.elo_pre_playoffs
                        .filter(|e| e.is_finite())
                        .map(|e| (*s, e))
                })
                .collect();
            if rated.len() < MIN_TEAMS {
                return None;
            }
            let teams = rated.len();
            let ratings: Vec<f64> = rated.iter().map(|(_, e)| *e).collect();
            let league_sum: f64 = ratings.iter().sum();
            let league_mean = league_sum / teams as f64;
            let league_sd = (ratings
                .iter()
                .map(|e| (e - league_mean).powi(2))
                .sum::<f64>()
                / teams as f64)
                .sqrt();
            let at_least = |result: PlayoffResult| -> Vec<f64> {
                rated
                    .iter()
                    .filter(|(s, _)| s.playoff_result >= result)
                    .map(|(_, e)| *e)
                    .collect()
            };
            let exactly = |result: PlayoffResult, count: usize| -> Option<Tier> {
                let members = at_least(result);
                (members.len() == count)
                    .then(|| tier(&members, league_mean, league_sd, league_sum, teams))
            };
            let champion = rated
                .iter()
                .find(|(s, _)| s.playoff_result == PlayoffResult::Champion);
            let field = at_least(PlayoffResult::FirstRound);
            Some(SeasonParity {
                season,
                league,
                teams,
                league_mean,
                league_sd,
                champion_code: champion.map(|(s, _)| s.code.clone()),
                champion_rank: champion
                    .map(|(_, c)| 1 + ratings.iter().filter(|e| **e > *c).count()),
                champion: exactly(PlayoffResult::Champion, 1),
                finalists: exactly(PlayoffResult::Finals, 2),
                final_four: exactly(PlayoffResult::ConfFinals, 4),
                playoff_field: (field.len() >= MIN_FIELD)
                    .then(|| tier(&field, league_mean, league_sd, league_sum, teams)),
            })
        })
        .collect()
}

/// The parity figures of a profile; `None` without a season.
pub fn summary(profile: &[SeasonParity]) -> Option<ParitySummary> {
    if profile.is_empty() {
        return None;
    }
    let zs = |pick: fn(&SeasonParity) -> Option<&Tier>| -> Option<f64> {
        mean(
            &profile
                .iter()
                .filter_map(pick)
                .map(|t| t.z)
                .collect::<Vec<_>>(),
        )
    };
    let ranks: Vec<f64> = profile
        .iter()
        .filter_map(|p| p.champion_rank)
        .map(|r| r as f64)
        .collect();
    Some(ParitySummary {
        seasons: profile.len(),
        seasons_with_champion: ranks.len(),
        mean_league_sd: profile.iter().map(|p| p.league_sd).sum::<f64>() / profile.len() as f64,
        champion_z: zs(|p| p.champion.as_ref()),
        finalists_z: zs(|p| p.finalists.as_ref()),
        final_four_z: zs(|p| p.final_four.as_ref()),
        playoff_field_z: zs(|p| p.playoff_field.as_ref()),
        champion_gap: mean(
            &profile
                .iter()
                .filter_map(|p| p.champion.as_ref())
                .map(|t| t.gap)
                .collect::<Vec<_>>(),
        ),
        champion_was_best: profile
            .iter()
            .filter(|p| p.champion_rank == Some(1))
            .count(),
        mean_champion_rank: mean(&ranks),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::models::GameType;
    use chrono::NaiveDate;

    fn stat(season: i32, code: &str, pre: Option<f64>, result: PlayoffResult) -> TeamSeasonStat {
        TeamSeasonStat {
            season: SeasonYear::of(season),
            code: code.to_string(),
            name: code.to_string(),
            franchise_key: code.to_string(),
            games: 82,
            peak: pre.unwrap_or(1500.0),
            peak_date: NaiveDate::from_ymd_opt(season, 4, 1).unwrap(),
            peak_game_type: GameType::Regular,
            low: 1400.0,
            low_date: NaiveDate::from_ymd_opt(season - 1, 11, 1).unwrap(),
            low_game_type: GameType::Regular,
            wins: 41,
            losses: 41,
            playoff_wins: 0,
            playoff_losses: 0,
            elo_start: Some(1500.0),
            elo_pre_playoffs: pre,
            elo_end: pre,
            playoff_result: result,
            conference: None,
            division: None,
        }
    }

    /// Eight teams: ratings 1400, 1420, ..., 1540 (mean 1470, SD about 45.8); the best wins.
    fn season(year: i32) -> Vec<TeamSeasonStat> {
        let results = [
            PlayoffResult::Missed,
            PlayoffResult::Missed,
            PlayoffResult::Missed,
            PlayoffResult::Missed,
            PlayoffResult::FirstRound,
            PlayoffResult::ConfFinals,
            PlayoffResult::Finals,
            PlayoffResult::Champion,
        ];
        (0..8)
            .map(|i| {
                stat(
                    year,
                    &format!("T{i}"),
                    Some(1400.0 + 20.0 * i as f64),
                    results[i],
                )
            })
            .collect()
    }

    #[test]
    fn the_tiers_are_measured_against_the_league_in_points_and_standard_deviations() {
        let stats = season(2010);
        let refs: Vec<&TeamSeasonStat> = stats.iter().collect();
        let p = profile(&refs);
        assert_eq!(p.len(), 1);
        let s = &p[0];
        assert_eq!((s.teams, s.league), (8, League::Nba));
        assert!((s.league_mean - 1470.0).abs() < 1e-9);
        let sd = (([0.0f64, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0]
            .iter()
            .map(|i| (1400.0 + 20.0 * i - 1470.0f64).powi(2))
            .sum::<f64>())
            / 8.0)
            .sqrt();
        assert!((s.league_sd - sd).abs() < 1e-9);
        let champion = s.champion.as_ref().unwrap();
        assert_eq!(
            (champion.teams, s.champion_code.as_deref(), s.champion_rank),
            (1, Some("T7"), Some(1))
        );
        assert!((champion.gap - 70.0).abs() < 1e-9);
        assert!((champion.z - 70.0 / sd).abs() < 1e-9);
        // The rest of the league averages 1460, so the gap to the rest is 80.
        assert!((champion.gap_to_rest - 80.0).abs() < 1e-9);
        let finalists = s.finalists.as_ref().unwrap();
        assert_eq!(finalists.teams, 2);
        assert!((finalists.mean - 1530.0).abs() < 1e-9);
        // Only three teams reached the conference finals here: no final four.
        assert!(s.final_four.is_none());
        let field = s.playoff_field.as_ref().unwrap();
        assert_eq!(field.teams, 4);
        assert!((field.gap - (1510.0 - 1470.0)).abs() < 1e-9);
        // Every z is positive and the tiers narrow upwards.
        assert!(field.z < finalists.z && finalists.z < champion.z);
    }

    #[test]
    fn small_seasons_unrated_teams_and_part_seasons_are_left_out() {
        let mut stats = season(2011);
        stats.truncate(7);
        let refs: Vec<&TeamSeasonStat> = stats.iter().collect();
        assert!(profile(&refs).is_empty());

        let mut stats = season(2012);
        stats.push(stat(2012, "T8", None, PlayoffResult::Missed));
        let mut folded = stat(2012, "T9", Some(1200.0), PlayoffResult::Missed);
        folded.games = 20;
        stats.push(folded);
        let refs: Vec<&TeamSeasonStat> = stats.iter().collect();
        let p = profile(&refs);
        assert_eq!(p[0].teams, 8);
        assert!((p[0].league_mean - 1470.0).abs() < 1e-9);

        // A champion whose rating is not the best ranks below 1; without a champion the
        // champion tier is absent and the summary counts no best-rated champion.
        let mut stats = season(2013);
        stats[7].playoff_result = PlayoffResult::Finals;
        stats[6].playoff_result = PlayoffResult::Champion;
        let refs: Vec<&TeamSeasonStat> = stats.iter().collect();
        let p = profile(&refs);
        assert_eq!(
            (p[0].champion_code.as_deref(), p[0].champion_rank),
            (Some("T6"), Some(2))
        );
        let mut stats = season(2014);
        stats[7].playoff_result = PlayoffResult::Finals;
        let refs: Vec<&TeamSeasonStat> = stats.iter().collect();
        let p = profile(&refs);
        assert!(p[0].champion.is_none() && p[0].champion_rank.is_none());
        assert_eq!(p[0].finalists.as_ref().unwrap().teams, 2);
        // A season whose archive labels only the champion has no field.
        let mut stats = season(2018);
        for s in stats.iter_mut().take(7) {
            s.playoff_result = PlayoffResult::Missed;
        }
        let refs: Vec<&TeamSeasonStat> = stats.iter().collect();
        let p = profile(&refs);
        assert!(p[0].champion.is_some() && p[0].playoff_field.is_none());
        // A season in progress has ratings but no postseason: no profile.
        let mut stats = season(2019);
        for s in stats.iter_mut() {
            s.playoff_result = PlayoffResult::Missed;
            s.games = 20;
        }
        let refs: Vec<&TeamSeasonStat> = stats.iter().collect();
        assert!(profile(&refs).is_empty());
    }

    #[test]
    fn the_two_leagues_of_one_season_are_profiled_apart() {
        // Eight NBA teams and eight ABA teams in 1971-72, each league with its own champion;
        // the ABA codes come from the reference list.
        let mut stats = season(1972);
        let aba = ["KEN", "INA", "UTS", "NYA", "VIR", "DNR", "FLO", "MMP"];
        for (i, code) in aba.iter().enumerate() {
            let result = if i == 7 {
                PlayoffResult::Champion
            } else if i >= 4 {
                PlayoffResult::FirstRound
            } else {
                PlayoffResult::Missed
            };
            stats.push(stat(1972, code, Some(1600.0 + 10.0 * i as f64), result));
        }
        let refs: Vec<&TeamSeasonStat> = stats.iter().collect();
        let p = profile(&refs);
        assert_eq!(
            p.iter().map(|x| x.league).collect::<Vec<_>>(),
            [League::Nba, League::Aba]
        );
        assert_eq!(p[1].champion_code.as_deref(), Some("MMP"));
        assert!((p[1].league_mean - 1635.0).abs() < 1e-9);
        assert!((p[0].league_mean - 1470.0).abs() < 1e-9);
    }

    #[test]
    fn the_summary_averages_the_seasons_that_have_each_tier() {
        let mut stats = season(2015);
        stats.extend(season(2016));
        // 2016: the champion came from the pack (rank 4).
        for s in stats
            .iter_mut()
            .filter(|s| s.season == SeasonYear::of(2016))
        {
            s.playoff_result = match s.code.as_str() {
                "T4" => PlayoffResult::Champion,
                "T7" => PlayoffResult::Finals,
                "T5" | "T6" => PlayoffResult::FirstRound,
                _ => PlayoffResult::Missed,
            };
        }
        let refs: Vec<&TeamSeasonStat> = stats.iter().collect();
        let p = profile(&refs);
        assert_eq!(p.len(), 2);
        let s = summary(&p).unwrap();
        assert_eq!(
            (s.seasons, s.seasons_with_champion, s.champion_was_best),
            (2, 2, 1)
        );
        assert!((s.mean_champion_rank.unwrap() - 2.5).abs() < 1e-9);
        let z = |year: i32| {
            p.iter()
                .find(|x| x.season.end_year() == year)
                .unwrap()
                .champion
                .as_ref()
                .unwrap()
                .z
        };
        assert!((s.champion_z.unwrap() - (z(2015) + z(2016)) / 2.0).abs() < 1e-9);
        assert!(s.final_four_z.is_none());
        assert!(summary(&[]).is_none());
    }
}
