//! Engine C: full-season Monte Carlo from an information cutoff date.
//!
//! Uses only games played on or before the cutoff. The remaining regular-season schedule is
//! simulated from Elo (optionally updating ratings game by game), standings are built with
//! the season's seeding rules, the play-in is resolved by a small state machine (actual
//! results are used when they happened on or before the cutoff), and the playoff bracket
//! is played out. Output: every team's probability of reaching the play-in, the playoffs,
//! each round, the Finals, and winning the title; its chance of each position and seed; and
//! the projected bracket, the most likely bracket slot by slot, with how rarely it occurred.

use std::collections::HashMap;

use chrono::NaiveDate;
use rand::Rng;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::domain::cup;
use crate::domain::elo;
use crate::domain::ids::{SeasonYear, TeamId};
use crate::domain::models::{Conference, Game, GameType, PlayoffResult, Round, Team};
use crate::domain::simulation::progress::{ChunkTracker, Progress};
use crate::domain::simulation::rng::{chunk_rng, chunks};
use crate::domain::simulation::stats;
use crate::domain::validation::{invalid, ValidationError};

use super::bracket::{
    play_bracket, Bracket, BracketTeam, EloModel, PlayoffFormat, RoundCounts, SeriesModel, SLOTS,
    SLOT_SEEDS,
};
use super::reconstruct;
use super::seeding::{self, Qualification};
use super::standings::{order_with_tiebreaks, Standing};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SeasonParams {
    pub season: SeasonYear,
    /// Information cutoff (inclusive). `None` = before the first game of the season.
    #[serde(default)]
    pub as_of: Option<NaiveDate>,
    pub iterations: u64,
    pub seed: u64,
    /// Home-court bonus of the simulated playoff games (and of the regular-season games
    /// when `regular_home_advantage` is not set).
    #[serde(default = "default_home_adv")]
    pub home_advantage: f64,
    /// Home-court bonus of the simulated regular-season and play-in games; the application
    /// fills it with the season's fitted value when a request leaves it out.
    #[serde(default)]
    pub regular_home_advantage: Option<f64>,
    /// Elo K-factor applied to simulated regular-season games (None = ratings fixed).
    #[serde(default = "default_k")]
    pub regular_season_k: Option<f64>,
    /// Elo K-factor applied to simulated playoff games (None = ratings fixed).
    #[serde(default)]
    pub playoff_k: Option<f64>,
    #[serde(default = "default_scale")]
    pub playoff_scale: f64,
}

fn default_home_adv() -> f64 {
    elo::DEFAULT_HOME_ADVANTAGE
}
fn default_k() -> Option<f64> {
    Some(20.0)
}
fn default_scale() -> f64 {
    1.0
}

impl SeasonParams {
    pub fn validate(&self, max_iterations: u64) -> Result<(), ValidationError> {
        if self.iterations == 0 || self.iterations > max_iterations {
            return invalid(
                "iterations",
                format!("iterations must be between 1 and {max_iterations}"),
            );
        }
        if !(0.0..=400.0).contains(&self.home_advantage) {
            return invalid(
                "home_advantage",
                "home advantage must be between 0 and 400 Elo points",
            );
        }
        if self
            .regular_home_advantage
            .is_some_and(|h| !(-100.0..=400.0).contains(&h))
        {
            return invalid(
                "regular_home_advantage",
                "the regular-season home advantage must be between -100 and 400 Elo points",
            );
        }
        for k in [self.regular_season_k, self.playoff_k]
            .into_iter()
            .flatten()
        {
            if !(0.0..=100.0).contains(&k) {
                return invalid("regular_season_k", "K-factors must be between 0 and 100");
            }
        }
        if !(0.1..=3.0).contains(&self.playoff_scale) {
            return invalid("playoff_scale", "playoff scale must be between 0.1 and 3");
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamForecast {
    pub team_id: TeamId,
    pub code: String,
    pub name: String,
    pub franchise_key: String,
    pub conference: Conference,
    pub division: String,
    pub elo_as_of: f64,
    pub wins: i32,
    pub losses: i32,
    pub remaining_games: i32,
    pub expected_wins: f64,
    pub p_top6: f64,
    pub p_play_in: f64,
    pub p_playoffs: f64,
    pub p_second_round: f64,
    pub p_conf_finals: f64,
    pub p_finals: f64,
    pub p_champion: f64,
    pub p_champion_se: f64,
    /// Probability of each conference finishing position (index 0 = 1st).
    pub position_distribution: Vec<f64>,
    /// Probability of holding each playoff seed, 1 to 8, after the play-in.
    #[serde(default)]
    pub seed_distribution: Vec<f64>,
    pub actual_result: Option<PlayoffResult>,
    pub actual_seed: Option<u8>,
}

/// A first-round series of the projected bracket.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FirstRoundSeries {
    pub slot_a: usize,
    pub slot_b: usize,
    /// The exact probability that slot a wins the series.
    pub p_a: f64,
}

/// The most likely bracket: for each conference and each seed in turn, the team most often
/// seeded there in the simulation among the teams not yet placed. One bracket of very many:
/// `p_exact` says how often this exact seeding occurred and `p_field` how often these
/// sixteen teams made the playoffs in any seeding, both over the first
/// [`BRACKET_SAMPLE`] simulated seasons. The probabilities through the bracket are exact,
/// at the ratings of the cutoff date with fixed ratings (no playoff K-factor).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectedBracket {
    /// The bracket; a team's record is its projected final record (expected wins, rounded).
    pub bracket: Bracket,
    /// Per slot, the exact probabilities of reaching the second round, the conference
    /// finals, the Finals and winning the title through this bracket.
    pub exact: Vec<[f64; 4]>,
    pub first_round: Vec<FirstRoundSeries>,
    /// Simulated seasons the shares below were counted over.
    pub sampled: u64,
    /// Times exactly this seeding occurred among the sampled seasons.
    pub exact_count: u64,
    /// Times these sixteen teams were the field, in any seeding, among the sampled seasons.
    pub field_count: u64,
    /// Times the single most frequent seeding occurred among the sampled seasons.
    pub most_common_count: u64,
    /// `exact_count / sampled`.
    pub p_exact: f64,
    /// `field_count / sampled`.
    pub p_field: f64,
    /// `most_common_count / sampled`.
    pub p_most_common: f64,
}

/// The simulated seasons whose seedings and fields are counted: enough for the shares,
/// and a bound on the memory of the counts, which hold one entry per distinct seeding.
pub const BRACKET_SAMPLE: u64 = 200_000;

/// A team's line in its Cup group, from the group games played on or before the cutoff.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CupGroupLine {
    pub team_id: TeamId,
    pub code: String,
    pub wins: i32,
    pub losses: i32,
    pub diff: i32,
}

/// A Cup group. The letter is the application's, by the lowest team identifier in the
/// group, not the league's.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CupGroup {
    pub conference: Conference,
    pub label: String,
    pub teams: Vec<CupGroupLine>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CupTeamForecast {
    pub team_id: TeamId,
    pub code: String,
    pub conference: Conference,
    pub p_knockout: f64,
    pub p_semifinal: f64,
    pub p_final: f64,
    pub p_winner: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CupSlot {
    pub team_id: TeamId,
    pub code: String,
    pub conference: Conference,
    pub seed: u8,
    pub elo: f64,
}

/// A quarterfinal of the projected Cup bracket with the exact chance that slot a wins it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CupQuarterfinal {
    pub slot_a: usize,
    pub slot_b: usize,
    pub p_a: f64,
}

/// The most likely Cup knockout bracket: per conference and seed in turn, the team most
/// often seeded there among the teams not yet placed, with the exact chances through it at
/// the cutoff date's ratings and how often it occurred (see [`ProjectedBracket`]).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectedCup {
    /// Slots in [`cup::CUP_SLOT_SEEDS`] order per conference, East then West.
    pub slots: Vec<CupSlot>,
    /// Per slot, the exact chance of reaching the semifinal, the final and winning the Cup.
    pub exact: Vec<[f64; 3]>,
    pub quarterfinals: Vec<CupQuarterfinal>,
    pub sampled: u64,
    pub exact_count: u64,
    pub field_count: u64,
    pub most_common_count: u64,
    pub p_exact: f64,
    pub p_field: f64,
    pub p_most_common: f64,
}

/// The NBA Cup forecast of a season (see `domain::cup`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CupForecast {
    pub groups: Vec<CupGroup>,
    pub group_games_played: usize,
    pub group_games_total: usize,
    /// Simulated seasons in which the knockout could be formed.
    pub iterations: u64,
    pub teams: Vec<CupTeamForecast>,
    pub projected: Option<ProjectedCup>,
    /// The Cup winner when the final was played on or before the cutoff.
    pub actual_winner: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SeasonForecast {
    pub season: SeasonYear,
    pub as_of: NaiveDate,
    pub information_cutoff_note: String,
    pub qualification: Qualification,
    pub games_played: usize,
    pub games_remaining: usize,
    pub iterations: u64,
    pub seed: u64,
    pub params: SeasonParams,
    pub teams: Vec<TeamForecast>,
    /// `None` when no simulated season reached the playoffs.
    #[serde(default)]
    pub projected_bracket: Option<ProjectedBracket>,
    /// The NBA Cup forecast; `None` with `cup_note` saying why.
    #[serde(default)]
    pub cup: Option<CupForecast>,
    #[serde(default)]
    pub cup_note: Option<String>,
    pub elapsed_ms: u128,
}

#[derive(Debug, Clone, thiserror::Error)]
pub enum SeasonError {
    #[error("season has no games")]
    NoGames,
    #[error("season {0} has no conference alignment data (supported from 2002-03)")]
    NoAlignment(SeasonYear),
}

// ---------------------------------------------------------------------------------------
// Play-in state machine
// ---------------------------------------------------------------------------------------

/// Team index within the simulation's team table.
type Ti = usize;

/// Progress of the play-in stage of one simulated season. `pending_game` says which game is
/// next; `advance` consumes its result. Seeds 7/8 come out of `Done`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayInState {
    /// 2021+: 7 hosts 8 for the 7th seed.
    SevenVsEight {
        s7: Ti,
        s8: Ti,
        s9: Ti,
        s10: Ti,
    },
    /// 2021+: 9 hosts 10, loser eliminated.
    NineVsTen {
        seed7: Ti,
        loser78: Ti,
        s9: Ti,
        s10: Ti,
    },
    /// 2021+: loser of 7/8 hosts winner of 9/10 for the 8th seed.
    LoserVsWinner {
        seed7: Ti,
        loser78: Ti,
        winner910: Ti,
    },
    /// 2020 bubble: 9th within four games of 8th; the 9th seed must win twice (neutral floor).
    BubbleFirst {
        s8: Ti,
        s9: Ti,
    },
    BubbleSecond {
        s8: Ti,
        s9: Ti,
    },
    Done {
        seed7: Ti,
        seed8: Ti,
    },
}

impl PlayInState {
    /// (home, away, neutral) of the next game, if any.
    pub const fn pending_game(self) -> Option<(Ti, Ti, bool)> {
        match self {
            PlayInState::SevenVsEight { s7, s8, .. } => Some((s7, s8, false)),
            PlayInState::NineVsTen { s9, s10, .. } => Some((s9, s10, false)),
            PlayInState::LoserVsWinner {
                loser78, winner910, ..
            } => Some((loser78, winner910, false)),
            PlayInState::BubbleFirst { s8, s9 } | PlayInState::BubbleSecond { s8, s9 } => {
                Some((s8, s9, true))
            }
            PlayInState::Done { .. } => None,
        }
    }

    pub const fn advance(self, home_won: bool) -> PlayInState {
        match self {
            PlayInState::SevenVsEight { s7, s8, s9, s10 } => {
                let (seed7, loser78) = if home_won { (s7, s8) } else { (s8, s7) };
                PlayInState::NineVsTen {
                    seed7,
                    loser78,
                    s9,
                    s10,
                }
            }
            PlayInState::NineVsTen {
                seed7,
                loser78,
                s9,
                s10,
            } => PlayInState::LoserVsWinner {
                seed7,
                loser78,
                winner910: if home_won { s9 } else { s10 },
            },
            PlayInState::LoserVsWinner {
                seed7,
                loser78,
                winner910,
            } => PlayInState::Done {
                seed7,
                seed8: if home_won { loser78 } else { winner910 },
            },
            PlayInState::BubbleFirst { s8, s9 } => {
                if home_won {
                    PlayInState::Done {
                        seed7: usize::MAX,
                        seed8: s8,
                    }
                } else {
                    PlayInState::BubbleSecond { s8, s9 }
                }
            }
            PlayInState::BubbleSecond { s8, s9 } => PlayInState::Done {
                seed7: usize::MAX,
                seed8: if home_won { s8 } else { s9 },
            },
            done @ PlayInState::Done { .. } => done,
        }
    }

    /// Every team that takes part in this play-in.
    pub fn participants(self) -> impl Iterator<Item = Ti> {
        let v: [Option<Ti>; 4] = match self {
            PlayInState::SevenVsEight { s7, s8, s9, s10 } => {
                [Some(s7), Some(s8), Some(s9), Some(s10)]
            }
            PlayInState::BubbleFirst { s8, s9 } => [Some(s8), Some(s9), None, None],
            _ => [None; 4],
        };
        v.into_iter().flatten()
    }
}

// ---------------------------------------------------------------------------------------
// Setup (done once) and per-iteration state
// ---------------------------------------------------------------------------------------

struct Remaining {
    home: Ti,
    away: Ti,
    neutral: bool,
    /// The index of the Cup group game this game is, when it is one.
    cup_game: Option<usize>,
    /// A game on a knockout date: its simulated result also decides the Cup game.
    knockout_game: bool,
}

/// A Cup group game: played with its result (home won, the winner's margin), or to come.
struct CupGroupGame {
    home: Ti,
    away: Ti,
    played: Option<(bool, i32)>,
}

/// The Cup of the season, from its dates and the schedule.
struct CupSetup {
    groups: Vec<cup::Group>,
    group_games: Vec<CupGroupGame>,
    /// Decided games on the knockout dates, the final included: home, away, home won.
    knockout_played: Vec<(Ti, Ti, bool)>,
    /// The winner of the final when it was played on or before the cutoff.
    final_winner: Option<Ti>,
}

impl CupSetup {
    /// The decided game between `a` and `b` on the knockout dates, as its winner.
    fn played_between(&self, a: Ti, b: Ti) -> Option<Ti> {
        self.knockout_played
            .iter()
            .find(|&&(h, v, _)| (h == a && v == b) || (h == b && v == a))
            .map(|&(h, v, home_won)| if home_won { h } else { v })
    }
}

struct PlayedPlayIn {
    home: Ti,
    away: Ti,
    home_won: bool,
}

#[derive(Clone)]
struct Acc {
    iterations: u64,
    wins_sum: Vec<f64>,
    top6: Vec<u64>,
    play_in: Vec<u64>,
    playoffs: Vec<u64>,
    reached: Vec<[u64; 4]>,
    positions: Vec<[u64; 16]>,
    /// Times each team held each playoff seed.
    seeds: Vec<[u64; 8]>,
    /// Simulated seasons in the sample whose seedings and fields were counted.
    sampled: u64,
    /// Times each exact seeding (team index per slot) occurred in the sample.
    brackets: HashMap<[u8; SLOTS], u64>,
    /// Times each sixteen-team field (sorted per conference) occurred in the sample.
    fields: HashMap<[u8; SLOTS], u64>,
    /// The Cup: seasons with a knockout, and per team the times it reached each stage.
    cup_iterations: u64,
    cup_knockout: Vec<u64>,
    cup_semis: Vec<u64>,
    cup_final: Vec<u64>,
    cup_winner: Vec<u64>,
    cup_seeds: Vec<[u64; 4]>,
    cup_sampled: u64,
    cup_brackets: HashMap<[u8; cup::CUP_SLOTS], u64>,
    cup_fields: HashMap<[u8; cup::CUP_SLOTS], u64>,
}

impl Acc {
    fn new(n: usize) -> Self {
        Self {
            iterations: 0,
            wins_sum: vec![0.0; n],
            top6: vec![0; n],
            play_in: vec![0; n],
            playoffs: vec![0; n],
            reached: vec![[0; 4]; n],
            positions: vec![[0; 16]; n],
            seeds: vec![[0; 8]; n],
            sampled: 0,
            brackets: HashMap::new(),
            fields: HashMap::new(),
            cup_iterations: 0,
            cup_knockout: vec![0; n],
            cup_semis: vec![0; n],
            cup_final: vec![0; n],
            cup_winner: vec![0; n],
            cup_seeds: vec![[0; 4]; n],
            cup_sampled: 0,
            cup_brackets: HashMap::new(),
            cup_fields: HashMap::new(),
        }
    }
    fn merge(mut self, o: Acc) -> Acc {
        self.iterations += o.iterations;
        self.sampled += o.sampled;
        for (k, v) in o.brackets {
            *self.brackets.entry(k).or_insert(0) += v;
        }
        for (k, v) in o.fields {
            *self.fields.entry(k).or_insert(0) += v;
        }
        self.cup_iterations += o.cup_iterations;
        self.cup_sampled += o.cup_sampled;
        for (k, v) in o.cup_brackets {
            *self.cup_brackets.entry(k).or_insert(0) += v;
        }
        for (k, v) in o.cup_fields {
            *self.cup_fields.entry(k).or_insert(0) += v;
        }
        for i in 0..self.wins_sum.len() {
            self.wins_sum[i] += o.wins_sum[i];
            self.top6[i] += o.top6[i];
            self.play_in[i] += o.play_in[i];
            self.playoffs[i] += o.playoffs[i];
            for r in 0..4 {
                self.reached[i][r] += o.reached[i][r];
            }
            for p in 0..16 {
                self.positions[i][p] += o.positions[i][p];
            }
            for s in 0..8 {
                self.seeds[i][s] += o.seeds[i][s];
            }
            self.cup_knockout[i] += o.cup_knockout[i];
            self.cup_semis[i] += o.cup_semis[i];
            self.cup_final[i] += o.cup_final[i];
            self.cup_winner[i] += o.cup_winner[i];
            for s in 0..4 {
                self.cup_seeds[i][s] += o.cup_seeds[i][s];
            }
        }
        self
    }
}

/// The keys of a Cup knockout bracket: the exact slots and the field in index order per
/// conference.
fn cup_keys(slots: &[Ti; cup::CUP_SLOTS]) -> Option<([u8; cup::CUP_SLOTS], [u8; cup::CUP_SLOTS])> {
    if slots.iter().any(|&t| t > u8::MAX as usize) {
        return None;
    }
    let mut exact = [0u8; cup::CUP_SLOTS];
    for (k, &t) in exact.iter_mut().zip(slots) {
        *k = t as u8;
    }
    let mut field = exact;
    field[..4].sort_unstable();
    field[4..].sort_unstable();
    Some((exact, field))
}

/// The most likely Cup slots, by the rule of [`most_likely_slots`] over four seeds.
fn most_likely_cup_slots(seeds: &[[u64; 4]], conf: &[Conference]) -> Option<[Ti; cup::CUP_SLOTS]> {
    let knockouts = |i: usize| seeds[i].iter().sum::<u64>();
    let mut per_conf = [[0usize; 4]; 2];
    for (ci, c) in [Conference::East, Conference::West].into_iter().enumerate() {
        let mut placed = vec![false; seeds.len()];
        for seed_idx in 0..4 {
            let unplaced = || (0..seeds.len()).filter(|&i| conf[i] == c && !placed[i]);
            let best = unplaced()
                .filter(|&i| seeds[i][seed_idx] > 0)
                .max_by_key(|&i| (seeds[i][seed_idx], std::cmp::Reverse(i)))
                .or_else(|| {
                    unplaced()
                        .filter(|&i| knockouts(i) > 0)
                        .max_by_key(|&i| (knockouts(i), std::cmp::Reverse(i)))
                })?;
            placed[best] = true;
            per_conf[ci][seed_idx] = best;
        }
    }
    Some(cup::slots_of(per_conf[0], per_conf[1]))
}

/// A simulated margin for a group game, as the winner's margin: a draw from the normal
/// distribution around the Elo expectation (home minus away), conditional on the side the
/// game went to, so a favourite that lost loses by a small margin more often than by a
/// large one. Drawn by rejection; after many rejections the size of a draw is kept.
fn draw_margin<R: Rng>(rng: &mut R, home_edge: f64, home_won: bool) -> i32 {
    let expected = home_edge * cup::POINTS_PER_ELO;
    let mut margin = 0.0;
    for attempt in 0..64 {
        let (u1, u2): (f64, f64) = (rng.gen::<f64>().max(1e-12), rng.gen());
        let normal = (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos();
        margin = expected + cup::MARGIN_SD * normal;
        if (margin > 0.0) == home_won || attempt == 63 {
            break;
        }
    }
    (margin.abs().round() as i32).max(1)
}

/// The keys of a simulated bracket: the exact seeding (team per slot) and the field (the
/// eight teams of each conference in index order). Slots 0 to 7 are the East and 8 to 15
/// the West, as the simulation and [`most_likely_slots`] both fill them. `None` when a
/// team index does not fit.
fn bracket_keys(slot_team: &[Ti]) -> Option<([u8; SLOTS], [u8; SLOTS])> {
    if slot_team.len() != SLOTS || slot_team.iter().any(|&t| t > u8::MAX as usize) {
        return None;
    }
    let mut exact = [0u8; SLOTS];
    for (k, &t) in exact.iter_mut().zip(slot_team) {
        *k = t as u8;
    }
    let mut field = exact;
    field[..8].sort_unstable();
    field[8..].sort_unstable();
    Some((exact, field))
}

/// The most likely slots: for each conference and each seed in turn, the team of that
/// conference most often seeded there among the teams not yet placed, in slot order (East
/// then West, slots in [`SLOT_SEEDS`] order). When every team that held a seed is already
/// placed, the seed goes to the unplaced team that made the playoffs most often. `None`
/// when no simulated season reached the playoffs.
fn most_likely_slots(seeds: &[[u64; 8]], conf: &[Conference]) -> Option<Vec<Ti>> {
    let playoffs = |i: usize| seeds[i].iter().sum::<u64>();
    let mut out = Vec::with_capacity(SLOTS);
    for c in [Conference::East, Conference::West] {
        let mut by_seed = [usize::MAX; 8];
        let mut placed = vec![false; seeds.len()];
        for seed_idx in 0..8 {
            let unplaced = || (0..seeds.len()).filter(|&i| conf[i] == c && !placed[i]);
            let best = unplaced()
                .filter(|&i| seeds[i][seed_idx] > 0)
                .max_by_key(|&i| (seeds[i][seed_idx], std::cmp::Reverse(i)))
                .or_else(|| {
                    unplaced()
                        .filter(|&i| playoffs(i) > 0)
                        .max_by_key(|&i| (playoffs(i), std::cmp::Reverse(i)))
                })?;
            placed[best] = true;
            by_seed[seed_idx] = best;
        }
        out.extend(SLOT_SEEDS.iter().map(|&s| by_seed[s as usize - 1]));
    }
    Some(out)
}

struct Setup {
    n: usize,
    ids: Vec<TeamId>,
    codes: Vec<String>,
    names: Vec<String>,
    fran: Vec<String>,
    conf: Vec<Conference>,
    div: Vec<String>,
    elo: Vec<f64>,
    base_wins: Vec<i32>,
    base_losses: Vec<i32>,
    base_pd: Vec<i32>,
    /// [conf_wins, conf_losses, div_wins, div_losses] per team from played games
    base_records: Vec<[i32; 4]>,
    /// n*n: wins of i over j
    base_h2h: Vec<i32>,
    remaining: Vec<Remaining>,
    played_play_in: Vec<PlayedPlayIn>,
    qualification: Qualification,
    format: PlayoffFormat,
    cup: Option<CupSetup>,
    /// Why there is no Cup, when there is none.
    cup_note: Option<String>,
}

impl Setup {
    fn build(
        params: &SeasonParams,
        games: &[Game],
        teams: &HashMap<TeamId, Team>,
        as_of: NaiveDate,
    ) -> Result<Setup, SeasonError> {
        let alignment = reconstruct::alignment_map(params.season, teams, games);
        if alignment.len() < 16 {
            return Err(SeasonError::NoAlignment(params.season));
        }
        let mut ids: Vec<TeamId> = alignment.keys().copied().collect();
        ids.sort_unstable();
        let n = ids.len();
        let idx: HashMap<TeamId, Ti> = ids.iter().enumerate().map(|(i, &t)| (t, i)).collect();
        let mut elo = vec![f64::NAN; n];
        let mut first_pre = vec![f64::NAN; n];
        let mut base_wins = vec![0; n];
        let mut base_losses = vec![0; n];
        let mut base_pd = vec![0; n];
        let mut base_records = vec![[0; 4]; n];
        let mut base_h2h = vec![0; n * n];
        let conf_of: Vec<Conference> = ids.iter().map(|t| alignment[t].0).collect();
        let div_of: Vec<&str> = ids.iter().map(|t| alignment[t].1.as_str()).collect();
        let mut remaining = Vec::new();
        let mut played_play_in = Vec::new();
        let dates = cup::dates_for(params.season);
        let knockout_dates: Vec<NaiveDate> = dates
            .as_ref()
            .map(|d| d.knockout_dates())
            .unwrap_or_default();
        let mut group_pairs: Vec<(Ti, Ti)> = Vec::new();
        let mut group_games: Vec<CupGroupGame> = Vec::new();
        let mut knockout_played = Vec::new();
        let mut final_winner = None;
        for g in games {
            let (Some(&h), Some(&a)) = (idx.get(&g.home_team_id), idx.get(&g.away_team_id)) else {
                continue;
            };
            let known = g.game_date <= as_of;
            let on_knockout_date = dates.is_some() && knockout_dates.contains(&g.game_date);
            let cup_game = match (&dates, g.game_type) {
                (Some(d), GameType::Regular) if d.group_nights.contains(&g.game_date) => {
                    group_pairs.push((h, a));
                    group_games.push(CupGroupGame {
                        home: h,
                        away: a,
                        played: g
                            .outcome()
                            .filter(|_| known)
                            .map(|o| (o.home_won(), o.margin())),
                    });
                    Some(group_games.len() - 1)
                }
                (Some(_), GameType::Regular) if known && on_knockout_date => {
                    if let Some(o) = g.outcome() {
                        knockout_played.push((h, a, o.home_won()));
                    }
                    None
                }
                (Some(_), GameType::CupFinal) if known => {
                    if let Some(o) = g.outcome() {
                        knockout_played.push((h, a, o.home_won()));
                        final_winner = Some(if o.home_won() { h } else { a });
                    }
                    None
                }
                _ => None,
            };
            if first_pre[h].is_nan() {
                first_pre[h] = g.home_elo_pre;
            }
            if first_pre[a].is_nan() {
                first_pre[a] = g.away_elo_pre;
            }
            match (known, g.outcome(), g.game_type) {
                (true, Some(o), GameType::Regular) => {
                    elo[h] = g.home_elo_post;
                    elo[a] = g.away_elo_post;
                    let (w, l) = if o.home_won() { (h, a) } else { (a, h) };
                    base_wins[w] += 1;
                    base_losses[l] += 1;
                    base_pd[w] += o.margin();
                    base_pd[l] -= o.margin();
                    base_h2h[w * n + l] += 1;
                    if conf_of[w] == conf_of[l] {
                        base_records[w][0] += 1;
                        base_records[l][1] += 1;
                        if div_of[w] == div_of[l] {
                            base_records[w][2] += 1;
                            base_records[l][3] += 1;
                        }
                    }
                }
                (true, Some(o), GameType::PlayIn) => {
                    elo[h] = g.home_elo_post;
                    elo[a] = g.away_elo_post;
                    played_play_in.push(PlayedPlayIn {
                        home: h,
                        away: a,
                        home_won: o.home_won(),
                    });
                }
                (true, Some(_), GameType::CupFinal) => {
                    elo[h] = g.home_elo_post;
                    elo[a] = g.away_elo_post;
                }
                (_, _, GameType::Regular) => remaining.push(Remaining {
                    home: h,
                    away: a,
                    neutral: g.neutral,
                    cup_game,
                    knockout_game: on_knockout_date,
                }),
                _ => {}
            }
        }
        let (cup, cup_note) = match dates {
            None if params.season < cup::CUP_SEASONS[0].season => {
                (None, Some("The NBA Cup began in 2023-24.".to_string()))
            }
            None => (
                None,
                Some(format!(
                    "No Cup dates are recorded for {}; add them to domain::cup.",
                    params.season.label()
                )),
            ),
            Some(_) => match cup::groups_from_games(&conf_of, &group_pairs) {
                Ok(groups) => (
                    Some(CupSetup {
                        groups,
                        group_games,
                        knockout_played,
                        final_winner,
                    }),
                    None,
                ),
                Err(e) => (None, Some(format!("No Cup forecast: {e}."))),
            },
        };
        if cup.is_none() {
            // The group games were marked while reading the schedule; without a Cup they
            // are ordinary games.
            for g in &mut remaining {
                g.cup_game = None;
                g.knockout_game = false;
            }
        }
        for (e, first) in elo.iter_mut().zip(&first_pre) {
            if e.is_nan() {
                *e = if first.is_nan() { 1500.0 } else { *first };
            }
        }
        let mut format = PlayoffFormat::for_season(params.season);
        format.neutral_site |= params.season.end_year() == 2020;
        Ok(Setup {
            n,
            codes: ids.iter().map(|t| teams[t].code.clone()).collect(),
            names: ids.iter().map(|t| teams[t].name.clone()).collect(),
            fran: ids
                .iter()
                .map(|t| teams[t].franchise_key().to_string())
                .collect(),
            conf: ids.iter().map(|t| alignment[t].0).collect(),
            div: ids.iter().map(|t| alignment[t].1.clone()).collect(),
            ids,
            elo,
            base_wins,
            base_losses,
            base_pd,
            base_records,
            base_h2h,
            remaining,
            played_play_in,
            qualification: seeding::qualification(params.season),
            format,
            cup,
            cup_note,
        })
    }

    /// Order a conference by record with tiebreaks (group head-to-head, point differential, random key).
    fn order_conference(&self, conf: Conference, st: &IterState) -> Vec<Standing<Ti>> {
        let n = self.n;
        let rows: Vec<Standing<Ti>> = (0..n)
            .filter(|&i| self.conf[i] == conf)
            .map(|i| Standing {
                team_id: i,
                conference: conf,
                division: self.div[i].clone(),
                wins: st.wins[i],
                losses: st.losses[i],
                point_diff: self.base_pd[i],
                conf_wins: st.records[i][0],
                conf_losses: st.records[i][1],
                div_wins: st.records[i][2],
                div_losses: st.records[i][3],
            })
            .collect();
        order_with_tiebreaks(rows, |a, b| st.h2h[a * n + b], |t| st.keys[t])
    }

    /// Resolve a play-in game: the historical result if it was played before the cutoff,
    /// otherwise a draw from Elo.
    fn resolve_game<R: Rng>(
        &self,
        rng: &mut R,
        elo: &mut [f64],
        game: (Ti, Ti, bool),
        rules: GameRules,
    ) -> bool {
        let (home, away, neutral) = game;
        if let Some(p) = self
            .played_play_in
            .iter()
            .find(|g| (g.home == home && g.away == away) || (g.home == away && g.away == home))
        {
            return if p.home == home {
                p.home_won
            } else {
                !p.home_won
            };
        }
        let p = elo::game_probability(
            elo[home],
            elo[away],
            true,
            neutral,
            rules.home_advantage,
            1.0,
        );
        let home_won = rng.gen::<f64>() < p;
        if let Some(k) = rules.k {
            // The update expects the archive's bonus, as the real update of the game will.
            let expected = elo::game_probability(
                elo[home],
                elo[away],
                true,
                neutral,
                elo::DEFAULT_HOME_ADVANTAGE,
                1.0,
            );
            let (nh, na) = elo::update(elo[home], elo[away], expected, home_won, k);
            elo[home] = nh;
            elo[away] = na;
        }
        home_won
    }

    /// The eight playoff teams of a conference in seed order, resolving the play-in.
    fn qualify<R: Rng>(
        &self,
        rng: &mut R,
        elo: &mut [f64],
        order: &[Standing<Ti>],
        rules: GameRules,
        acc: &mut Acc,
    ) -> Vec<Ti> {
        let t = |i: usize| order[i].team_id;
        let initial = match self.qualification {
            Qualification::Top8 => return (0..8).map(t).collect(),
            Qualification::Bubble2020 => {
                let games_behind = f64::from(
                    (order[7].wins - order[8].wins) + (order[8].losses - order[7].losses),
                ) / 2.0;
                if games_behind <= 4.0 {
                    PlayInState::BubbleFirst { s8: t(7), s9: t(8) }
                } else {
                    return (0..8).map(t).collect();
                }
            }
            Qualification::PlayIn => PlayInState::SevenVsEight {
                s7: t(6),
                s8: t(7),
                s9: t(8),
                s10: t(9),
            },
        };
        for p in initial.participants() {
            acc.play_in[p] += 1;
        }
        let mut state = initial;
        while let Some(game) = state.pending_game() {
            let home_won = self.resolve_game(rng, elo, game, rules);
            state = state.advance(home_won);
        }
        let PlayInState::Done { seed7, seed8 } = state else {
            unreachable!("loop exits on Done")
        };
        let direct = if seed7 == usize::MAX { 7 } else { 6 };
        let mut out: Vec<Ti> = (0..direct).map(t).collect();
        if seed7 != usize::MAX {
            out.push(seed7);
        }
        out.push(seed8);
        out
    }
}

/// Per-game Elo rules for simulated regular-season and play-in games.
#[derive(Debug, Clone, Copy)]
struct GameRules {
    home_advantage: f64,
    k: Option<f64>,
}

/// Mutable state of one simulated season (reset from `Setup` each iteration).
struct IterState {
    wins: Vec<i32>,
    losses: Vec<i32>,
    /// [conf_wins, conf_losses, div_wins, div_losses] per team
    records: Vec<[i32; 4]>,
    h2h: Vec<i32>,
    keys: Vec<u64>,
    elo: Vec<f64>,
    /// The Cup group games of this season: played results, then simulated ones.
    cup_results: Vec<Option<(bool, i32)>>,
    /// Games on the knockout dates simulated as regular-season games this iteration:
    /// home, away, home won; the Cup keeps their results.
    knockout_sim: Vec<(Ti, Ti, bool)>,
}

/// The played results of the Cup group games, as the start of every iteration.
fn cup_played(setup: &Setup) -> Vec<Option<(bool, i32)>> {
    setup
        .cup
        .as_ref()
        .map(|c| c.group_games.iter().map(|g| g.played).collect())
        .unwrap_or_default()
}

impl IterState {
    fn new(setup: &Setup) -> Self {
        Self {
            wins: setup.base_wins.clone(),
            losses: setup.base_losses.clone(),
            records: setup.base_records.clone(),
            h2h: setup.base_h2h.clone(),
            keys: vec![0; setup.n],
            elo: setup.elo.clone(),
            cup_results: cup_played(setup),
            knockout_sim: Vec::new(),
        }
    }
    fn reset<R: Rng>(&mut self, setup: &Setup, rng: &mut R) {
        self.wins.copy_from_slice(&setup.base_wins);
        self.losses.copy_from_slice(&setup.base_losses);
        self.records.copy_from_slice(&setup.base_records);
        self.h2h.copy_from_slice(&setup.base_h2h);
        self.elo.copy_from_slice(&setup.elo);
        self.keys.iter_mut().for_each(|k| *k = rng.gen());
        if let Some(c) = &setup.cup {
            for (r, g) in self.cup_results.iter_mut().zip(&c.group_games) {
                *r = g.played;
            }
        }
        self.knockout_sim.clear();
    }
    /// Record a decided game between team indices `w` (winner) and `l` (loser).
    fn record(&mut self, setup: &Setup, w: Ti, l: Ti) {
        self.wins[w] += 1;
        self.losses[l] += 1;
        self.h2h[w * setup.n + l] += 1;
        if setup.conf[w] == setup.conf[l] {
            self.records[w][0] += 1;
            self.records[l][1] += 1;
            if setup.div[w] == setup.div[l] {
                self.records[w][2] += 1;
                self.records[l][3] += 1;
            }
        }
    }
}

/// Simulated seasons per work chunk.
const CHUNK: u64 = 250;

/// One Cup: the group tables from the played and simulated group games, the knockout
/// seeds, then the quarterfinals at the higher seed, the semifinals and the final on a
/// neutral floor, each decided by its played result when it was played on or before the
/// cutoff, by this iteration's result when the schedule already holds it and the
/// regular-season loop simulated it, and by a draw from Elo otherwise. A knockout game the
/// schedule does not hold yet feeds neither the standings nor the ratings.
fn simulate_cup<R: Rng>(
    c: &CupSetup,
    st: &mut IterState,
    rng: &mut R,
    rules: GameRules,
    in_sample: bool,
    acc: &mut Acc,
) {
    let results: Vec<cup::GroupResult> = c
        .group_games
        .iter()
        .zip(&st.cup_results)
        .filter_map(|(g, r)| {
            r.map(|(home_won, margin)| cup::GroupResult {
                home: g.home,
                away: g.away,
                home_won,
                margin,
            })
        })
        .collect();
    let key = |t: Ti| st.keys[t];
    let tables: Vec<(Conference, Vec<cup::GroupLine>)> = c
        .groups
        .iter()
        .map(|g| (g.conference, cup::group_table(&g.teams, &results, key)))
        .collect();
    let (Some(east), Some(west)) = (
        cup::knockout_seeds(&tables, Conference::East, key),
        cup::knockout_seeds(&tables, Conference::West, key),
    ) else {
        return;
    };
    let slots = cup::slots_of(east, west);
    acc.cup_iterations += 1;
    for (s, &t) in slots.iter().enumerate() {
        acc.cup_knockout[t] += 1;
        acc.cup_seeds[t][cup::CUP_SLOT_SEEDS[s % 4] as usize - 1] += 1;
    }
    if in_sample {
        if let Some((exact, field)) = cup_keys(&slots) {
            acc.cup_sampled += 1;
            *acc.cup_brackets.entry(exact).or_insert(0) += 1;
            *acc.cup_fields.entry(field).or_insert(0) += 1;
        }
    }
    let decide = |a: Ti, b: Ti, a_home: Option<bool>, rng: &mut R| -> Ti {
        if let Some(w) = c.played_between(a, b) {
            return w;
        }
        if let Some(&(h, v, home_won)) = st
            .knockout_sim
            .iter()
            .find(|&&(h, v, _)| (h == a && v == b) || (h == b && v == a))
        {
            return if home_won { h } else { v };
        }
        let p = elo::game_probability(
            st.elo[a],
            st.elo[b],
            a_home.unwrap_or(true),
            a_home.is_none(),
            rules.home_advantage,
            1.0,
        );
        if rng.gen::<f64>() < p {
            a
        } else {
            b
        }
    };
    // Quarterfinals: slot pairs (0,1), (2,3), (4,5), (6,7); the higher seed hosts.
    let mut qf = [0 as Ti; 4];
    for (k, pair) in slots.chunks(2).enumerate() {
        let (a, b) = (pair[0], pair[1]);
        qf[k] = decide(a, b, Some(cup::hosts_quarterfinal(2 * k, 2 * k + 1)), rng);
        acc.cup_semis[qf[k]] += 1;
    }
    let east_final = decide(qf[0], qf[1], None, rng);
    let west_final = decide(qf[2], qf[3], None, rng);
    acc.cup_final[east_final] += 1;
    acc.cup_final[west_final] += 1;
    let winner = decide(east_final, west_final, None, rng);
    acc.cup_winner[winner] += 1;
}

/// The Cup forecast from the counts, with the projected knockout bracket.
fn cup_forecast(c: &CupSetup, setup: &Setup, acc: &Acc, home_adv: f64) -> CupForecast {
    let played: Vec<cup::GroupResult> = c
        .group_games
        .iter()
        .filter_map(|g| {
            g.played.map(|(home_won, margin)| cup::GroupResult {
                home: g.home,
                away: g.away,
                home_won,
                margin,
            })
        })
        .collect();
    let mut letters = [0usize; 2];
    let groups: Vec<CupGroup> = c
        .groups
        .iter()
        .map(|g| {
            let ci = g.conference.index();
            let label = format!(
                "{} {}",
                match g.conference {
                    Conference::East => "East",
                    Conference::West => "West",
                },
                (b'A' + letters[ci] as u8) as char
            );
            letters[ci] += 1;
            let table = cup::group_table(&g.teams, &played, |t| u64::MAX - t as u64);
            CupGroup {
                conference: g.conference,
                label,
                teams: table
                    .iter()
                    .map(|l| CupGroupLine {
                        team_id: setup.ids[l.team],
                        code: setup.codes[l.team].clone(),
                        wins: l.wins,
                        losses: l.losses,
                        diff: l.diff,
                    })
                    .collect(),
            }
        })
        .collect();
    let it = acc.cup_iterations.max(1) as f64;
    let mut teams: Vec<CupTeamForecast> = (0..setup.n)
        .map(|i| CupTeamForecast {
            team_id: setup.ids[i],
            code: setup.codes[i].clone(),
            conference: setup.conf[i],
            p_knockout: acc.cup_knockout[i] as f64 / it,
            p_semifinal: acc.cup_semis[i] as f64 / it,
            p_final: acc.cup_final[i] as f64 / it,
            p_winner: acc.cup_winner[i] as f64 / it,
        })
        .collect();
    teams.sort_by(|a, b| {
        b.p_winner
            .partial_cmp(&a.p_winner)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.code.cmp(&b.code))
    });
    let projected = most_likely_cup_slots(&acc.cup_seeds, &setup.conf).map(|slots| {
        // A knockout game played on or before the cutoff, the final included, is decided.
        let p = |a: usize, b: usize, a_home: Option<bool>| {
            let (ta, tb) = (slots[a], slots[b]);
            if let Some(w) = c.played_between(ta, tb) {
                return if w == ta { 1.0 } else { 0.0 };
            }
            elo::game_probability(
                setup.elo[ta],
                setup.elo[tb],
                a_home.unwrap_or(true),
                a_home.is_none(),
                home_adv,
                1.0,
            )
        };
        let exact = cup::exact(p);
        let quarterfinals = (0..cup::CUP_SLOTS)
            .step_by(2)
            .map(|a| CupQuarterfinal {
                slot_a: a,
                slot_b: a + 1,
                p_a: p(a, a + 1, Some(cup::hosts_quarterfinal(a, a + 1))),
            })
            .collect();
        let (exact_count, field_count) = cup_keys(&slots)
            .map(|(e, f)| {
                (
                    acc.cup_brackets.get(&e).copied().unwrap_or(0),
                    acc.cup_fields.get(&f).copied().unwrap_or(0),
                )
            })
            .unwrap_or((0, 0));
        let most_common_count = acc.cup_brackets.values().copied().max().unwrap_or(0);
        let sampled = acc.cup_sampled;
        let share = |n: u64| n as f64 / sampled.max(1) as f64;
        ProjectedCup {
            slots: slots
                .iter()
                .enumerate()
                .map(|(s, &t)| CupSlot {
                    team_id: setup.ids[t],
                    code: setup.codes[t].clone(),
                    conference: setup.conf[t],
                    seed: cup::CUP_SLOT_SEEDS[s % 4],
                    elo: setup.elo[t],
                })
                .collect(),
            exact: exact.to_vec(),
            quarterfinals,
            sampled,
            exact_count,
            field_count,
            most_common_count,
            p_exact: share(exact_count),
            p_field: share(field_count),
            p_most_common: share(most_common_count),
        }
    });
    CupForecast {
        groups,
        group_games_played: c.group_games.iter().filter(|g| g.played.is_some()).count(),
        group_games_total: c.group_games.len(),
        iterations: acc.cup_iterations,
        teams,
        projected,
        actual_winner: c.final_winner.map(|w| setup.codes[w].clone()),
    }
}

/// Games of team `i` still to play.
fn remaining_of(setup: &Setup, i: Ti) -> i32 {
    setup
        .remaining
        .iter()
        .filter(|g| g.home == i || g.away == i)
        .count() as i32
}

pub fn run<P: Progress + ?Sized>(
    params: &SeasonParams,
    games: &[Game],
    teams: &HashMap<TeamId, Team>,
    progress: &P,
) -> Result<SeasonForecast, SeasonError> {
    let start = std::time::Instant::now();
    let first_date = games
        .first()
        .map(|g| g.game_date)
        .ok_or(SeasonError::NoGames)?;
    let first_playoff = games.iter().find(|g| g.is_playoff()).map(|g| g.game_date);
    let mut as_of = params
        .as_of
        .unwrap_or(first_date - chrono::Duration::days(1));
    let mut note = format!("Only games on or before {as_of} inform ratings and standings.");
    if let Some(fp) = first_playoff.filter(|fp| as_of >= *fp) {
        as_of = fp - chrono::Duration::days(1);
        note = format!("Cutoff moved to {as_of}, the day before the playoffs started; playoff games are never used as inputs.");
    }
    let setup = Setup::build(params, games, teams, as_of)?;
    let n = setup.n;
    let games_played = games
        .iter()
        .filter(|g| g.game_date <= as_of && !g.is_playoff() && g.is_played())
        .count();
    // Regular-season and play-in games take the fitted bonus; the playoffs keep their own.
    let home_adv = params
        .regular_home_advantage
        .unwrap_or(params.home_advantage);
    let rules = GameRules {
        home_advantage: home_adv,
        k: params.regular_season_k,
    };
    let playoff_model = EloModel {
        home_advantage: params.home_advantage,
        scale: params.playoff_scale,
        k: params.playoff_k,
    };
    let chunk_list = chunks(params.iterations, CHUNK);
    let tracker = ChunkTracker::new(progress, chunk_list.len() as u64);

    let acc = chunk_list
        .par_iter()
        .map(|&(idx, count)| {
            let mut acc = Acc::new(n);
            if tracker.is_cancelled() {
                return acc;
            }
            let mut rng = chunk_rng(params.seed, idx);
            let mut st = IterState::new(&setup);
            let mut reached: RoundCounts = [[0; SLOTS]; 4];
            // Seedings and fields are counted over the first BRACKET_SAMPLE seasons only.
            let in_sample = idx * CHUNK < BRACKET_SAMPLE;
            for _ in 0..count {
                st.reset(&setup, &mut rng);
                // remaining regular season
                for g in &setup.remaining {
                    let p = elo::game_probability(
                        st.elo[g.home],
                        st.elo[g.away],
                        true,
                        g.neutral,
                        home_adv,
                        1.0,
                    );
                    let home_won = rng.gen::<f64>() < p;
                    if let Some(ci) = g.cup_game {
                        let edge = st.elo[g.home] - st.elo[g.away]
                            + if g.neutral { 0.0 } else { home_adv };
                        st.cup_results[ci] =
                            Some((home_won, draw_margin(&mut rng, edge, home_won)));
                    }
                    if g.knockout_game {
                        st.knockout_sim.push((g.home, g.away, home_won));
                    }
                    if let Some(k) = params.regular_season_k {
                        // The update expects the archive's bonus, as the real update will.
                        let expected = elo::game_probability(
                            st.elo[g.home],
                            st.elo[g.away],
                            true,
                            g.neutral,
                            elo::DEFAULT_HOME_ADVANTAGE,
                            1.0,
                        );
                        let (nh, na) =
                            elo::update(st.elo[g.home], st.elo[g.away], expected, home_won, k);
                        st.elo[g.home] = nh;
                        st.elo[g.away] = na;
                    }
                    let (w, l) = if home_won {
                        (g.home, g.away)
                    } else {
                        (g.away, g.home)
                    };
                    st.record(&setup, w, l);
                }
                for i in 0..n {
                    acc.wins_sum[i] += f64::from(st.wins[i]);
                }
                if let Some(c) = &setup.cup {
                    simulate_cup(c, &mut st, &mut rng, rules, in_sample, &mut acc);
                }
                // standings, seeding and qualification per conference
                let mut bracket_teams: Vec<BracketTeam> = Vec::with_capacity(SLOTS);
                let mut slot_team: Vec<Ti> = Vec::with_capacity(SLOTS);
                let mut ok = true;
                for conf in [Conference::East, Conference::West] {
                    let ordered = setup.order_conference(conf, &st);
                    let seeded = seeding::seed_order(params.season, &ordered);
                    for (pos, s) in seeded.iter().enumerate().take(16) {
                        let i = s.team_id;
                        acc.positions[i][pos] += 1;
                        if pos < 6 {
                            acc.top6[i] += 1;
                        }
                    }
                    if seeded.len() < 10 {
                        ok = false;
                        break;
                    }
                    let qualified = setup.qualify(&mut rng, &mut st.elo, &seeded, rules, &mut acc);
                    let mut conf_slots: [Option<(Ti, BracketTeam)>; 8] = Default::default();
                    for (seed_idx, &ti) in qualified.iter().enumerate() {
                        acc.playoffs[ti] += 1;
                        acc.seeds[ti][seed_idx] += 1;
                        let seed = (seed_idx + 1) as u8;
                        let p = SLOT_SEEDS
                            .iter()
                            .position(|&s| s == seed)
                            .expect("seed 1-8");
                        conf_slots[p] = Some((
                            ti,
                            BracketTeam {
                                team_id: setup.ids[ti],
                                code: String::new(),
                                name: String::new(),
                                franchise_key: String::new(),
                                conference: conf,
                                seed,
                                elo: st.elo[ti],
                                wins: st.wins[ti],
                                losses: st.losses[ti],
                                win_pct: f64::from(st.wins[ti])
                                    / f64::from((st.wins[ti] + st.losses[ti]).max(1)),
                                actual_result: None,
                            },
                        ));
                    }
                    for (ti, team) in conf_slots.into_iter().map(|s| s.expect("eight qualifiers")) {
                        slot_team.push(ti);
                        bracket_teams.push(team);
                    }
                }
                if !ok {
                    continue;
                }
                if in_sample {
                    if let Some((exact, field)) = bracket_keys(&slot_team) {
                        acc.sampled += 1;
                        *acc.brackets.entry(exact).or_insert(0) += 1;
                        *acc.fields.entry(field).or_insert(0) += 1;
                    }
                }
                let bracket = Bracket {
                    season: params.season,
                    teams: bracket_teams,
                    format: setup.format,
                    actual_series: vec![],
                    notes: vec![],
                };
                let mut b_elos: Vec<f64> = bracket.teams.iter().map(|t| t.elo).collect();
                reached.iter_mut().flatten().for_each(|x| *x = 0);
                play_bracket(
                    &bracket,
                    &mut rng,
                    &playoff_model,
                    &mut b_elos,
                    &mut reached,
                );
                for (slot, &ti) in slot_team.iter().enumerate() {
                    for (total, round_counts) in acc.reached[ti].iter_mut().zip(&reached) {
                        *total += round_counts[slot];
                    }
                }
                acc.iterations += 1;
            }
            tracker.chunk_done();
            acc
        })
        .reduce(|| Acc::new(n), Acc::merge);

    let it = acc.iterations.max(1) as f64;
    // actual outcomes for comparison (never used as inputs)
    let series = reconstruct::series_from_games(params.season, games);
    let actual_results = reconstruct::playoff_results(&series);
    let actual_seed: HashMap<TeamId, u8> = reconstruct::reconstruct(params.season, games, teams)
        .map(|r| {
            r.bracket
                .teams
                .iter()
                .map(|t| (t.team_id, t.seed))
                .collect()
        })
        .unwrap_or_default();
    let projected_bracket = most_likely_slots(&acc.seeds, &setup.conf).map(|slots| {
        let games_total =
            |i: Ti| setup.base_wins[i] + setup.base_losses[i] + remaining_of(&setup, i);
        let teams_in: Vec<BracketTeam> = slots
            .iter()
            .enumerate()
            .map(|(slot, &i)| {
                let wins = (acc.wins_sum[i] / it).round() as i32;
                let losses = (games_total(i) - wins).max(0);
                BracketTeam {
                    team_id: setup.ids[i],
                    code: setup.codes[i].clone(),
                    name: setup.names[i].clone(),
                    franchise_key: setup.fran[i].clone(),
                    conference: setup.conf[i],
                    seed: SLOT_SEEDS[slot % 8],
                    elo: setup.elo[i],
                    wins,
                    losses,
                    win_pct: f64::from(wins) / f64::from((wins + losses).max(1)),
                    actual_result: None,
                }
            })
            .collect();
        let bracket = Bracket {
            season: params.season,
            teams: teams_in,
            format: setup.format,
            actual_series: vec![],
            notes: vec![],
        };
        let exact_model = EloModel {
            home_advantage: params.home_advantage,
            scale: params.playoff_scale,
            k: None,
        };
        let exact = bracket.exact_probabilities(&exact_model);
        let elos: Vec<f64> = bracket.teams.iter().map(|t| t.elo).collect();
        let first_round = (0..SLOTS)
            .step_by(2)
            .map(|a| FirstRoundSeries {
                slot_a: a,
                slot_b: a + 1,
                p_a: exact_model.series_probability(&bracket, a, a + 1, Round::First, &elos),
            })
            .collect();
        let (exact_count, field_count) = bracket_keys(&slots)
            .map(|(e, f)| {
                (
                    acc.brackets.get(&e).copied().unwrap_or(0),
                    acc.fields.get(&f).copied().unwrap_or(0),
                )
            })
            .unwrap_or((0, 0));
        let most_common_count = acc.brackets.values().copied().max().unwrap_or(0);
        let sampled = acc.sampled;
        let share = |c: u64| c as f64 / sampled.max(1) as f64;
        ProjectedBracket {
            bracket,
            exact,
            first_round,
            sampled,
            exact_count,
            field_count,
            most_common_count,
            p_exact: share(exact_count),
            p_field: share(field_count),
            p_most_common: share(most_common_count),
        }
    });
    let mut teams_out: Vec<TeamForecast> = (0..n)
        .map(|i| {
            let p_champ = acc.reached[i][3] as f64 / it;
            TeamForecast {
                team_id: setup.ids[i],
                code: setup.codes[i].clone(),
                name: setup.names[i].clone(),
                franchise_key: setup.fran[i].clone(),
                conference: setup.conf[i],
                division: setup.div[i].clone(),
                elo_as_of: setup.elo[i],
                wins: setup.base_wins[i],
                losses: setup.base_losses[i],
                remaining_games: remaining_of(&setup, i),
                expected_wins: acc.wins_sum[i] / it,
                p_top6: acc.top6[i] as f64 / it,
                p_play_in: acc.play_in[i] as f64 / it,
                p_playoffs: acc.playoffs[i] as f64 / it,
                p_second_round: acc.reached[i][0] as f64 / it,
                p_conf_finals: acc.reached[i][1] as f64 / it,
                p_finals: acc.reached[i][2] as f64 / it,
                p_champion: p_champ,
                p_champion_se: stats::binomial_se(p_champ, acc.iterations),
                position_distribution: acc.positions[i].iter().map(|&c| c as f64 / it).collect(),
                seed_distribution: acc.seeds[i].iter().map(|&c| c as f64 / it).collect(),
                actual_result: actual_results.get(&setup.ids[i]).copied(),
                actual_seed: actual_seed.get(&setup.ids[i]).copied(),
            }
        })
        .collect();
    teams_out.sort_by(|a, b| {
        b.p_champion
            .partial_cmp(&a.p_champion)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.code.cmp(&b.code))
    });
    Ok(SeasonForecast {
        season: params.season,
        as_of,
        information_cutoff_note: note,
        qualification: setup.qualification,
        games_played,
        games_remaining: setup.remaining.len(),
        iterations: acc.iterations,
        seed: params.seed,
        params: params.clone(),
        teams: teams_out,
        projected_bracket,
        cup: setup
            .cup
            .as_ref()
            .map(|c| cup_forecast(c, &setup, &acc, home_adv)),
        cup_note: setup.cup_note.clone(),
        elapsed_ms: start.elapsed().as_millis(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn play_in_tournament_state_machine() {
        let s = PlayInState::SevenVsEight {
            s7: 7,
            s8: 8,
            s9: 9,
            s10: 10,
        };
        assert_eq!(s.participants().collect::<Vec<_>>(), vec![7, 8, 9, 10]);
        assert_eq!(s.pending_game(), Some((7, 8, false)));
        let s = s.advance(false); // 8 beats 7 -> 8 is the 7 seed
        assert_eq!(s.pending_game(), Some((9, 10, false)));
        let s = s.advance(true); // 9 beats 10
        assert_eq!(s.pending_game(), Some((7, 9, false)));
        let s = s.advance(false); // 9 beats 7
        assert_eq!(s, PlayInState::Done { seed7: 8, seed8: 9 });
        assert_eq!(s.pending_game(), None);
        assert_eq!(s.advance(true), s);
    }

    #[test]
    fn bubble_play_in_state_machine() {
        let s = PlayInState::BubbleFirst { s8: 1, s9: 2 };
        assert_eq!(
            s.advance(true),
            PlayInState::Done {
                seed7: usize::MAX,
                seed8: 1
            }
        );
        let s = s.advance(false);
        assert_eq!(s, PlayInState::BubbleSecond { s8: 1, s9: 2 });
        assert_eq!(s.pending_game(), Some((1, 2, true)));
        assert_eq!(
            s.advance(false),
            PlayInState::Done {
                seed7: usize::MAX,
                seed8: 2
            }
        );
    }
    #[test]
    fn the_most_likely_slots_take_each_seed_in_turn_without_repeating_a_team() {
        // Four East and four West teams would be too few; sixteen teams, two conferences.
        let conf: Vec<Conference> = (0..16)
            .map(|i| {
                if i < 8 {
                    Conference::East
                } else {
                    Conference::West
                }
            })
            .collect();
        // Team i of each conference is most often seed i+1, but team 0 also leads seed 2,
        // so seed 2 goes to the next team not yet placed.
        let mut seeds = vec![[0u64; 8]; 16];
        for c in 0..2 {
            for i in 0..8 {
                seeds[c * 8 + i][i] = 100;
            }
            seeds[c * 8][1] = 150;
            seeds[c * 8 + 1][1] = 90;
        }
        let slots = most_likely_slots(&seeds, &conf).unwrap();
        assert_eq!(slots.len(), SLOTS);
        let mut sorted = slots.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), SLOTS, "a team sits in one slot");
        // Slot order follows SLOT_SEEDS: slot 0 is seed 1 (team 0), slot 1 is seed 8.
        assert_eq!((slots[0], slots[1], slots[4]), (0, 7, 1));
        assert_eq!(slots[8], 8);
        // Team 3 held seed 2 only (never seed 4). Seed 4 then has no unplaced holder, so it
        // goes to the unplaced team that qualified most often (team 4, 100 times), and the
        // seeds after it follow the same rule until team 3, which qualified 50 times,
        // takes seed 8: every team that ever qualified is placed once.
        seeds[3] = [0, 50, 0, 0, 0, 0, 0, 0];
        let slots = most_likely_slots(&seeds, &conf).unwrap();
        let mut east: Vec<Ti> = slots[..8].to_vec();
        east.sort_unstable();
        assert_eq!(east, (0..8).collect::<Vec<_>>());
        assert_eq!(slots[1], 3, "seed 8 is team 3");
        // Only seven East teams ever qualified: no full bracket.
        seeds[3] = [0; 8];
        assert!(most_likely_slots(&seeds, &conf).is_none());
        // The keys: the exact seeding and the field in index order per conference.
        let (exact, field) = bracket_keys(&slots).unwrap();
        assert_eq!(exact[1], 3);
        assert_eq!(&field[..8], &[0, 1, 2, 3, 4, 5, 6, 7]);
        assert!(bracket_keys(&slots[..15]).is_none());
    }
    /// A synthetic 2023-24 season: thirty teams with their real codes, six groups of five
    /// whose games fall on the Cup's group nights, one earlier regular game, and, when
    /// asked, every group game played with the home team (the lower index) winning by five
    /// and the final played between the first East and the first West team.
    fn cup_season(
        season: i32,
        cross_conference: bool,
        played: bool,
    ) -> (HashMap<TeamId, Team>, Vec<Game>) {
        use crate::domain::ids::FranchiseId;
        use crate::domain::models::{EloSource, ResultSource};
        let codes = [
            "ATL", "BOS", "BRK", "CHO", "CHI", "CLE", "DET", "IND", "MIA", "MIL", "NYK", "ORL",
            "PHI", "TOR", "WAS", "DAL", "DEN", "GSW", "HOU", "LAC", "LAL", "MEM", "MIN", "NOP",
            "OKC", "PHO", "POR", "SAC", "SAS", "UTA",
        ];
        let teams: HashMap<TeamId, Team> = codes
            .iter()
            .enumerate()
            .map(|(i, code)| {
                let id = TeamId::new(i as i32 + 1);
                (
                    id,
                    Team {
                        id,
                        code: code.to_string(),
                        franchise_id: FranchiseId::new(i as i32 + 1),
                        name: code.to_string(),
                        first_season: None,
                        last_season: None,
                    },
                )
            })
            .collect();
        let d = |s: &str| NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap();
        let game = |seq: i16,
                    date: NaiveDate,
                    home: usize,
                    away: usize,
                    game_type: GameType,
                    score: Option<(i32, i32)>| Game {
            id: i64::from(seq),
            season: SeasonYear::of(season),
            game_date: date,
            seq,
            home_team_id: TeamId::new(home as i32 + 1),
            away_team_id: TeamId::new(away as i32 + 1),
            neutral: false,
            game_type,
            home_score: score.map(|s| s.0),
            away_score: score.map(|s| s.1),
            home_elo_pre: 1500.0 + 5.0 * home as f64,
            away_elo_pre: 1500.0 + 5.0 * away as f64,
            home_elo_post: 1500.0 + 5.0 * home as f64,
            away_elo_post: 1500.0 + 5.0 * away as f64,
            home_win_prob: None,
            elo_source: EloSource::Computed,
            result_source: ResultSource::Archive,
            ref_home_elo_pre: None,
            ref_away_elo_pre: None,
            ref_home_elo_post: None,
            ref_away_elo_post: None,
        };
        let nights = [
            "2023-11-03",
            "2023-11-10",
            "2023-11-14",
            "2023-11-17",
            "2023-11-21",
            "2023-11-24",
            "2023-11-28",
        ];
        let mut games = vec![game(
            0,
            d("2023-10-24"),
            0,
            15,
            GameType::Regular,
            Some((100, 95)),
        )];
        let mut seq = 1;
        for g in 0..6 {
            let base = g * 5;
            for a in 0..5 {
                for b in a + 1..5 {
                    let (home, away) = if cross_conference && seq == 1 {
                        (0, 15)
                    } else {
                        (base + a, base + b)
                    };
                    let score = played.then_some((100, 95));
                    games.push(game(
                        seq,
                        d(nights[seq as usize % 7]),
                        home,
                        away,
                        GameType::Regular,
                        score,
                    ));
                    seq += 1;
                }
            }
        }
        if played {
            games.push(game(
                seq,
                d("2023-12-09"),
                0,
                15,
                GameType::CupFinal,
                Some((110, 100)),
            ));
        }
        games.sort_by_key(|g| (g.game_date, g.seq));
        (teams, games)
    }

    fn cup_params(season: i32, as_of: &str) -> SeasonParams {
        SeasonParams {
            season: SeasonYear::of(season),
            as_of: Some(NaiveDate::parse_from_str(as_of, "%Y-%m-%d").unwrap()),
            iterations: 200,
            seed: 1,
            home_advantage: 100.0,
            regular_home_advantage: None,
            regular_season_k: None,
            playoff_k: None,
            playoff_scale: 1.0,
        }
    }

    #[test]
    fn the_cup_is_forecast_from_the_groups_the_schedule_makes() {
        use crate::domain::simulation::progress::Silent;
        let (teams, games) = cup_season(2024, false, false);
        let r = run(&cup_params(2024, "2023-11-01"), &games, &teams, &Silent).unwrap();
        let c = r.cup.as_ref().expect("a Cup forecast");
        assert_eq!(r.cup_note, None);
        assert_eq!(
            (c.group_games_played, c.group_games_total, c.iterations),
            (0, 60, 200)
        );
        assert_eq!(c.groups.len(), 6);
        assert_eq!(c.groups[0].label, "East A");
        assert_eq!(
            c.groups[0]
                .teams
                .iter()
                .map(|t| t.code.as_str())
                .collect::<Vec<_>>(),
            ["ATL", "BOS", "BRK", "CHO", "CHI"]
        );
        assert_eq!(c.groups[3].label, "West A");
        assert!((c.teams.iter().map(|t| t.p_winner).sum::<f64>() - 1.0).abs() < 1e-9);
        assert!((c.teams.iter().map(|t| t.p_knockout).sum::<f64>() - 8.0).abs() < 1e-9);
        for t in &c.teams {
            assert!(t.p_winner <= t.p_final + 1e-12 && t.p_final <= t.p_semifinal + 1e-12);
            assert!(t.p_semifinal <= t.p_knockout + 1e-12);
        }
        let p = c.projected.as_ref().expect("a projected bracket");
        let mut ids: Vec<TeamId> = p.slots.iter().map(|s| s.team_id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), 8);
        assert!((p.exact.iter().map(|e| e[2]).sum::<f64>() - 1.0).abs() < 1e-9);
        assert_eq!(p.quarterfinals.len(), 4);
        assert!(p.most_common_count >= 1 && p.field_count >= p.exact_count);
        assert_eq!(c.actual_winner, None);
    }

    #[test]
    fn a_played_cup_keeps_its_results_and_a_broken_schedule_leaves_a_note() {
        use crate::domain::simulation::progress::Silent;
        // Every group game played, the final too: the winner is known and a team that
        // never left its group has no chance; the stage chances stay ordered.
        let (teams, games) = cup_season(2024, false, true);
        let r = run(&cup_params(2024, "2024-01-05"), &games, &teams, &Silent).unwrap();
        let c = r.cup.as_ref().unwrap();
        assert_eq!(
            (c.group_games_played, c.actual_winner.as_deref()),
            (60, Some("ATL"))
        );
        // ATL won its group (4-0) in every season; it is the lowest-rated team, so it
        // loses most simulated semifinals, and the played final counts only when it is
        // there against DAL.
        let atl = c.teams.iter().find(|t| t.code == "ATL").unwrap();
        assert!((atl.p_knockout - 1.0).abs() < 1e-12);
        assert!(atl.p_winner > 0.0 && atl.p_winner <= atl.p_final + 1e-12);
        let was = c.teams.iter().find(|t| t.code == "WAS").unwrap();
        assert_eq!(was.p_knockout, 0.0);
        for t in &c.teams {
            assert!(t.p_winner <= t.p_final + 1e-12 && t.p_final <= t.p_semifinal + 1e-12);
        }
        assert!(
            (
                c.groups[0].teams[0].code.as_str(),
                c.groups[0].teams[0].wins
            ) == ("ATL", 4)
        );
        // A group game across conferences breaks the shape: no Cup, a note, no panic.
        let (teams, games) = cup_season(2024, true, false);
        let r = run(&cup_params(2024, "2023-11-01"), &games, &teams, &Silent).unwrap();
        assert!(r.cup.is_none());
        assert!(r
            .cup_note
            .as_deref()
            .unwrap()
            .contains("across conferences"));
        assert!(r.projected_bracket.is_some());
        // A season before the Cup.
        let (teams, games) = cup_season(2016, false, false);
        let r = run(&cup_params(2016, "2023-11-01"), &games, &teams, &Silent).unwrap();
        assert_eq!(r.cup_note.as_deref(), Some("The NBA Cup began in 2023-24."));
        // The margin draw sits on the winner's side and is at least a point.
        let mut rng = chunk_rng(7, 0);
        for _ in 0..200 {
            assert!(draw_margin(&mut rng, 280.0, false) >= 1);
            assert!(draw_margin(&mut rng, -280.0, true) >= 1);
        }
        let small: Vec<i32> = (0..400)
            .map(|_| draw_margin(&mut rng, 280.0, false))
            .collect();
        let big: Vec<i32> = (0..400)
            .map(|_| draw_margin(&mut rng, 280.0, true))
            .collect();
        let mean = |v: &[i32]| v.iter().sum::<i32>() as f64 / v.len() as f64;
        assert!(
            mean(&small) < mean(&big),
            "an upset is a closer game than a favourite's win"
        );
    }
}
