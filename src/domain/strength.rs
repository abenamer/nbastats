//! The strength a title record implies, in Elo terms.
//!
//! The Alpha Simulator reads a record backwards into an alpha, a ratio of title odds. This
//! module reads the same record into a rating: the gap above the league mean that a team of
//! constant strength would need, playing the actual playoff fields of the window's seasons,
//! for its title chance per season to equal the record's. The gap is reported as Elo points,
//! as points per game (FiveThirtyEight's 100 Elo for 3.5 points) and as the record such a
//! team would post over 82 games against an average opponent.
//!
//! Each season's field is the pre-playoff rating of every other team. The team of interest is
//! placed at the mean plus the gap, the top sixteen by rating make a modern 16-team bracket
//! seeded by rating (the two strongest in different halves), and the bracket's exact
//! probabilities give the title chance. The chance rises with the gap, so bisection inverts
//! it. Everything here is exact; nothing is simulated.

use serde::Serialize;

use crate::domain::elo;
use crate::domain::ids::{SeasonYear, TeamId};
use crate::domain::models::Conference;
use crate::domain::playoffs::bracket::{
    slot_for, Bracket, BracketTeam, EloModel, FinalsPattern, PlayoffFormat, SLOTS,
};
use crate::domain::simulation::exact::Strength;

/// Elo points per point of margin: 100 Elo for 3.5 points.
pub const ELO_PER_POINT: f64 = 100.0 / 3.5;
/// The widest gap searched; a team this far above the league wins the title all but surely.
const MAX_GAP: f64 = 1500.0;
const MIN_GAP: f64 = -800.0;

/// One season's field: the pre-playoff ratings of every team but the one asked about, and
/// the mean of the whole league that season.
#[derive(Debug, Clone, PartialEq)]
pub struct Field {
    pub season: SeasonYear,
    pub mean: f64,
    pub others: Vec<f64>,
}

/// A rating gap above the league mean, read three ways.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EloReading {
    /// Elo points above the league mean.
    pub gap: f64,
    /// The title chance a team at this gap has against the fields. The chance steps where
    /// the team overtakes another and changes seed, so it can differ from the chance asked
    /// for; the page shows it when it does.
    pub title_chance: f64,
    /// The rating itself, mean plus gap.
    pub rating: f64,
    /// Points per game better than an average team.
    pub points_per_game: f64,
    /// Chance to beat an average team in one game on a neutral floor.
    pub game_win_probability: f64,
    /// Wins over 82 games against average opponents, half at home.
    pub expected_wins_82: f64,
}

/// The record read into ratings against the fields of the window.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EloStrength {
    pub seasons: Vec<SeasonYear>,
    pub league_mean: f64,
    pub home_advantage: f64,
    /// The gap at which the record is the expected outcome; `None` for a title every season.
    pub implied: Option<EloReading>,
    /// The gaps at the ends of the record's 95 percent range.
    pub low: Option<EloReading>,
    pub high: Option<EloReading>,
    /// The gap at which the record becomes a coin flip.
    pub even_odds: Option<EloReading>,
    /// The title chance of an average team (gap 0) against these fields.
    pub p_average: f64,
}

/// A modern 16-team bracket from a field with the team of interest at `rating`, and the slot
/// the team holds; `None` when the team does not make the top sixteen.
fn bracket_with(field: &Field, rating: f64) -> Option<(Bracket, usize)> {
    let mut ranked: Vec<(f64, bool)> = field.others.iter().map(|&r| (r, false)).collect();
    ranked.push((rating, true));
    // The team of interest ranks ahead of a team with the same rating.
    ranked.sort_by(|a, b| b.0.total_cmp(&a.0).then(b.1.cmp(&a.1)));
    ranked.truncate(SLOTS);
    if ranked.len() < SLOTS || !ranked.iter().any(|(_, mine)| *mine) {
        return None;
    }
    let mut target_slot = 0;
    let mut by_slot: Vec<(usize, BracketTeam)> = ranked
        .into_iter()
        .enumerate()
        .map(|(rank, (elo, mine))| {
            // Ranks alternate between the halves, so the two strongest meet only in the
            // Finals; rank r is seed r / 2 + 1 of its half, a one-to-one map onto the slots.
            let conference = Conference::from_index(rank % 2);
            let seed = (rank / 2 + 1) as u8;
            let slot = slot_for(conference, seed);
            if mine {
                target_slot = slot;
            }
            let wins = 82 - rank as i32;
            let label = format!("S{}", rank + 1);
            (
                slot,
                BracketTeam {
                    team_id: TeamId::new(rank as i32 + 1),
                    code: label.clone(),
                    name: format!("Seed {}", rank + 1),
                    franchise_key: label,
                    conference,
                    seed,
                    elo,
                    wins,
                    losses: 82 - wins,
                    win_pct: wins as f64 / 82.0,
                    actual_result: None,
                },
            )
        })
        .collect();
    by_slot.sort_by_key(|(slot, _)| *slot);
    Some((
        Bracket {
            season: field.season,
            teams: by_slot.into_iter().map(|(_, team)| team).collect(),
            format: PlayoffFormat {
                first_round_games: 7,
                finals_pattern: FinalsPattern::TwoTwoOneOneOne,
                neutral_site: false,
            },
            actual_series: Vec::new(),
            notes: Vec::new(),
        },
        target_slot,
    ))
}

/// The title chance of a team `gap` above the mean, averaged over the fields.
pub fn title_chance(gap: f64, fields: &[Field], home_advantage: f64) -> f64 {
    if fields.is_empty() {
        return 0.0;
    }
    let model = EloModel {
        home_advantage,
        scale: 1.0,
        k: None,
    };
    let total: f64 = fields
        .iter()
        .map(|f| match bracket_with(f, f.mean + gap) {
            Some((bracket, slot)) => bracket.exact_probabilities(&model)[slot][3],
            None => 0.0,
        })
        .sum();
    total / fields.len() as f64
}

/// The gap at which the title chance reaches `p`; `None` when no gap in range does. The
/// chance rises with the gap apart from small steps where the team overtakes another and
/// changes seed, so the caller reads the chance the gap found actually has (see
/// [`reading`]).
pub fn gap_for_title_chance(p: f64, fields: &[Field], home_advantage: f64) -> Option<f64> {
    if fields.is_empty() || !(0.0..1.0).contains(&p) {
        return None;
    }
    if title_chance(MAX_GAP, fields, home_advantage) < p {
        return None;
    }
    // Forty-eight halvings of the 2,300-point range leave a precision far below a point.
    let (mut lo, mut hi) = (MIN_GAP, MAX_GAP);
    for _ in 0..48 {
        let mid = (lo + hi) / 2.0;
        if title_chance(mid, fields, home_advantage) < p {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    Some((lo + hi) / 2.0)
}

/// A gap read three ways against an average team, with the title chance it has against
/// `fields`.
pub fn reading(gap: f64, mean: f64, fields: &[Field], home_advantage: f64) -> EloReading {
    let rating = mean + gap;
    let at = |home: bool, neutral: bool| {
        elo::game_probability(rating, mean, home, neutral, home_advantage, 1.0)
    };
    EloReading {
        gap,
        title_chance: title_chance(gap, fields, home_advantage),
        rating,
        points_per_game: gap / ELO_PER_POINT,
        game_win_probability: at(false, true),
        expected_wins_82: 41.0 * (at(true, false) + at(false, false)),
    }
}

/// The record's strength in Elo terms against `fields`; `None` without a field.
pub fn elo_strength(
    record: &Strength,
    fields: &[Field],
    home_advantage: f64,
) -> Option<EloStrength> {
    if fields.is_empty() {
        return None;
    }
    let league_mean = fields.iter().map(|f| f.mean).sum::<f64>() / fields.len() as f64;
    let read = |p: f64| {
        gap_for_title_chance(p, fields, home_advantage)
            .map(|g| reading(g, league_mean, fields, home_advantage))
    };
    Some(EloStrength {
        seasons: fields.iter().map(|f| f.season).collect(),
        league_mean,
        home_advantage,
        implied: read(record.probability),
        low: read(record.probability_low),
        high: read(record.probability_high),
        even_odds: read(record.even_odds_probability),
        p_average: title_chance(0.0, fields, home_advantage),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::simulation::exact::strength_for_titles;
    use approx::assert_abs_diff_eq;

    fn flat(season: i32) -> Field {
        Field {
            season: SeasonYear::of(season),
            mean: 1505.0,
            others: vec![1505.0; 29],
        }
    }

    /// A field shaped like a modern league: the best at +250, a spread of about 120.
    fn spread(season: i32) -> Field {
        let others: Vec<f64> = (0..29)
            .map(|i| 1505.0 + 250.0 - 500.0 * i as f64 / 28.0)
            .collect();
        Field {
            season: SeasonYear::of(season),
            mean: 1505.0,
            others,
        }
    }

    #[test]
    fn against_equals_even_odds_need_about_118_elo() {
        // Seed 1 with home court every round: four series at p^4 = 0.5 means a series chance
        // of 0.841, a per-game chance of 0.663 on a neutral floor, a gap near 118 Elo.
        let fields = [flat(2016)];
        let gap = gap_for_title_chance(0.5, &fields, 100.0).unwrap();
        assert_abs_diff_eq!(gap, 118.0, epsilon = 2.0);
        assert_abs_diff_eq!(title_chance(gap, &fields, 100.0), 0.5, epsilon = 1e-6);
        // An average team among equals is seed 1 by the tie-break and still wins about 9%.
        assert!(title_chance(0.0, &fields, 100.0) > 0.05);
    }

    #[test]
    fn a_real_spread_asks_far_more() {
        let fields = [spread(2016), spread(2017)];
        let flat_gap = gap_for_title_chance(0.5, &[flat(2016)], 100.0).unwrap();
        let spread_gap = gap_for_title_chance(0.5, &fields, 100.0).unwrap();
        assert!(spread_gap > flat_gap + 80.0, "{spread_gap} vs {flat_gap}");
        // The chance rises with the gap and a team outside the top sixteen has none.
        assert!(title_chance(100.0, &fields, 100.0) < title_chance(200.0, &fields, 100.0));
        assert_eq!(title_chance(-400.0, &fields, 100.0), 0.0);
        // An average team is around the cut line of the top sixteen and rarely wins.
        assert!(title_chance(0.0, &fields, 100.0) < 0.02);
    }

    #[test]
    fn brackets_fill_every_slot_once_and_seed_by_rating() {
        let (bracket, slot) = bracket_with(&spread(2016), 1505.0 + 100.0).unwrap();
        assert_eq!(bracket.teams.len(), SLOTS);
        let mut codes: Vec<&str> = bracket.teams.iter().map(|t| t.code.as_str()).collect();
        codes.sort_unstable();
        codes.dedup();
        assert_eq!(codes.len(), SLOTS, "every seed once");
        for (i, team) in bracket.teams.iter().enumerate() {
            assert_eq!(slot_for(team.conference, team.seed), i, "slot order");
        }
        // The two strongest sit in different halves as the seed 1s.
        let east1 = &bracket.teams[slot_for(Conference::East, 1)];
        let west1 = &bracket.teams[slot_for(Conference::West, 1)];
        assert!(east1.elo >= west1.elo);
        assert!(west1.elo > bracket.teams[slot_for(Conference::East, 2)].elo);
        // The team of interest holds the slot reported.
        assert_eq!(bracket.teams[slot].elo, 1605.0);
        // Below the cut, no bracket; too small a field, no bracket and no gap.
        assert!(bracket_with(&spread(2016), 1000.0).is_none());
        let small = Field {
            season: SeasonYear::of(2016),
            mean: 1505.0,
            others: vec![1505.0; 10],
        };
        assert!(bracket_with(&small, 1600.0).is_none());
        assert_eq!(gap_for_title_chance(0.5, &[small], 100.0), None);
        assert_eq!(gap_for_title_chance(1.0, &[spread(2016)], 100.0), None);
    }

    #[test]
    fn the_chance_found_is_reported_with_the_gap() {
        // One field with a single dominant team: the chance steps when the target overtakes
        // it, so the reading carries the chance the gap really has.
        let mut others = vec![1505.0; 29];
        others[0] = 1505.0 + 300.0;
        let fields = [Field {
            season: SeasonYear::of(2016),
            mean: 1505.0,
            others,
        }];
        for p in [0.1, 0.25, 0.5, 0.75] {
            let gap = gap_for_title_chance(p, &fields, 100.0).unwrap();
            let r = reading(gap, 1505.0, &fields, 100.0);
            assert_abs_diff_eq!(
                r.title_chance,
                title_chance(gap, &fields, 100.0),
                epsilon = 1e-12
            );
            assert!(
                (r.title_chance - p).abs() <= 0.15,
                "{p} -> {}",
                r.title_chance
            );
        }
        // Against equals the chance found is the chance asked for.
        let flat = [flat(2016)];
        let gap = gap_for_title_chance(0.3, &flat, 100.0).unwrap();
        assert_abs_diff_eq!(
            reading(gap, 1505.0, &flat, 100.0).title_chance,
            0.3,
            epsilon = 1e-6
        );
    }

    #[test]
    fn readings_and_the_full_record() {
        let flat = [flat(2016)];
        let r = reading(0.0, 1505.0, &flat, 100.0);
        assert_abs_diff_eq!(r.expected_wins_82, 41.0, epsilon = 1e-9);
        assert_abs_diff_eq!(r.game_win_probability, 0.5, epsilon = 1e-12);
        let r = reading(100.0, 1505.0, &flat, 100.0);
        assert_abs_diff_eq!(r.points_per_game, 3.5, epsilon = 1e-9);
        assert!(r.expected_wins_82 > 50.0 && r.rating == 1605.0);
        // Four titles in eight seasons against a real spread: a gap in the hundreds, with the
        // low end of the range below it and the even-odds gap below the implied one.
        let record = strength_for_titles(30, 8, 4).unwrap();
        let fields: Vec<Field> = (2015..=2022).map(spread).collect();
        let s = elo_strength(&record, &fields, 100.0).unwrap();
        let implied = s.implied.unwrap();
        assert!(
            implied.gap > 150.0 && implied.gap < 400.0,
            "{}",
            implied.gap
        );
        assert!(s.low.unwrap().gap < implied.gap);
        assert!(s.high.unwrap().gap > implied.gap);
        assert!(s.even_odds.unwrap().gap < implied.gap);
        assert_eq!(s.seasons.len(), 8);
        // A title every season has no finite gap; no field, no answer.
        let sure = strength_for_titles(30, 3, 3).unwrap();
        assert!(elo_strength(&sure, &fields, 100.0)
            .unwrap()
            .implied
            .is_none());
        assert!(elo_strength(&record, &[], 100.0).is_none());
    }
}
