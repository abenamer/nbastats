//! A 16-team single-elimination bracket with best-of-N series.
//!
//! Slot layout: slots 0–7 are the East, 8–15 the West. Within a conference the positions
//! hold seeds `[1, 8, 4, 5, 2, 7, 3, 6]`, so round 1 pairs adjacent slots, round 2 pairs
//! adjacent pairs, and so on. This is the NBA's fixed (non-reseeding) bracket since 1984.
//!
//! Team strength enters through the [`GameModel`] trait (per-game probabilities) or the
//! [`SeriesModel`] trait (per-series probabilities). Every `GameModel` is a `SeriesModel`
//! through a blanket implementation that plays the series game by game, so the Elo and
//! log5 models share one exact DP and one simulator, and the whole thing is monomorphized.

use rand::Rng;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::elo;
use crate::models::{Conference, PlayoffResult, Round};
use crate::simulation::progress::{ChunkTracker, Progress};
use crate::simulation::rng::{chunk_rng, chunks};
use crate::simulation::stats;

pub const SLOT_SEEDS: [u8; 8] = [1, 8, 4, 5, 2, 7, 3, 6];
pub const SLOTS: usize = 16;

pub fn slot_for(conference: Conference, seed: u8) -> usize {
    let p = SLOT_SEEDS.iter().position(|&s| s == seed).expect("seed 1-8");
    conference.index() * 8 + p
}

pub fn conference_of(slot: usize) -> Conference {
    Conference::from_index(slot / 8)
}

// ---------------------------------------------------------------------------------------
// Series state machine
// ---------------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Side {
    A,
    B,
}

impl Side {
    pub const fn other(self) -> Side {
        match self {
            Side::A => Side::B,
            Side::B => Side::A,
        }
    }
}

/// A best-of-N series is either still being played or decided. `record` is the only way to
/// advance it, and a decided series ignores further games.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SeriesState {
    InProgress { wins_a: u8, wins_b: u8, need: u8 },
    Won { winner: Side, wins_a: u8, wins_b: u8 },
}

impl SeriesState {
    pub const fn best_of(games: u8) -> Self {
        SeriesState::InProgress { wins_a: 0, wins_b: 0, need: games / 2 + 1 }
    }

    pub const fn record(self, game_winner: Side) -> Self {
        match self {
            SeriesState::InProgress { wins_a, wins_b, need } => {
                let (wins_a, wins_b) = match game_winner {
                    Side::A => (wins_a + 1, wins_b),
                    Side::B => (wins_a, wins_b + 1),
                };
                if wins_a == need {
                    SeriesState::Won { winner: Side::A, wins_a, wins_b }
                } else if wins_b == need {
                    SeriesState::Won { winner: Side::B, wins_a, wins_b }
                } else {
                    SeriesState::InProgress { wins_a, wins_b, need }
                }
            }
            done @ SeriesState::Won { .. } => done,
        }
    }

    pub const fn games_played(self) -> u8 {
        match self {
            SeriesState::InProgress { wins_a, wins_b, .. } | SeriesState::Won { wins_a, wins_b, .. } => wins_a + wins_b,
        }
    }

    pub const fn winner(self) -> Option<Side> {
        match self {
            SeriesState::Won { winner, .. } => Some(winner),
            SeriesState::InProgress { .. } => None,
        }
    }

    pub const fn wins(self) -> (u8, u8) {
        match self {
            SeriesState::InProgress { wins_a, wins_b, .. } | SeriesState::Won { wins_a, wins_b, .. } => (wins_a, wins_b),
        }
    }
}

// ---------------------------------------------------------------------------------------
// Format and teams
// ---------------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum FinalsPattern {
    TwoThreeTwo,
    TwoTwoOneOneOne,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct PlayoffFormat {
    pub first_round_games: u8,
    pub finals_pattern: FinalsPattern,
    /// All games on a neutral floor (2020 bubble).
    pub neutral_site: bool,
}

impl PlayoffFormat {
    pub fn for_season(season: i32) -> Self {
        Self {
            first_round_games: if season >= 2003 { 7 } else { 5 },
            finals_pattern: if (1985..=2013).contains(&season) {
                FinalsPattern::TwoThreeTwo
            } else {
                FinalsPattern::TwoTwoOneOneOne
            },
            neutral_site: season == 2020,
        }
    }

    /// Home pattern for a round: `true` = the home-court team hosts.
    pub fn pattern(&self, round: Round) -> &'static [bool] {
        match (round, self.first_round_games, self.finals_pattern) {
            (Round::First, 5, _) => &elo::HOME_PATTERN_2_2_1,
            (Round::Finals, _, FinalsPattern::TwoThreeTwo) => &elo::HOME_PATTERN_2_3_2,
            _ => &elo::HOME_PATTERN_2_2_1_1_1,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BracketTeam {
    pub team_id: i32,
    pub code: String,
    pub name: String,
    pub franchise_key: String,
    pub conference: Conference,
    pub seed: u8,
    pub elo: f64,
    pub wins: i32,
    pub losses: i32,
    pub win_pct: f64,
    /// Historical outcome, when known.
    pub actual_result: Option<PlayoffResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SeriesRecord {
    pub round: Round,
    pub slot_a: usize,
    pub slot_b: usize,
    pub winner_slot: Option<usize>,
    pub wins_a: u8,
    pub wins_b: u8,
    pub game1_host_slot: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bracket {
    pub season: i32,
    pub teams: Vec<BracketTeam>,
    pub format: PlayoffFormat,
    pub actual_series: Vec<SeriesRecord>,
    pub notes: Vec<String>,
}

impl Bracket {
    /// Slot holding home-court advantage for a series between `a` and `b`.
    /// Uses the historical game-1 host when that series was actually played; otherwise the
    /// better regular-season record, then the better seed, then the East slot.
    pub fn home_court(&self, a: usize, b: usize) -> usize {
        if let Some(s) = self.actual_series.iter().find(|s| (s.slot_a == a && s.slot_b == b) || (s.slot_a == b && s.slot_b == a)) {
            return s.game1_host_slot;
        }
        let (ta, tb) = (&self.teams[a], &self.teams[b]);
        match ta.win_pct.partial_cmp(&tb.win_pct) {
            Some(std::cmp::Ordering::Greater) => a,
            Some(std::cmp::Ordering::Less) => b,
            _ if ta.seed != tb.seed => {
                if ta.seed < tb.seed {
                    a
                } else {
                    b
                }
            }
            _ => a.min(b),
        }
    }

    /// Round in which slots `a` and `b` would meet, if they can.
    pub fn meeting_round(a: usize, b: usize) -> Option<Round> {
        if a == b || a >= SLOTS || b >= SLOTS {
            return None;
        }
        Round::ALL.into_iter().find(|r| a >> r.number() == b >> r.number())
    }

    /// Exact per-slot probabilities of reaching each stage:
    /// `[p_second_round, p_conf_finals, p_finals, p_champion]`.
    pub fn exact_probabilities<M: SeriesModel>(&self, model: &M) -> Vec<[f64; 4]> {
        let elos: Vec<f64> = self.teams.iter().map(|t| t.elo).collect();
        let mut reach = vec![[1.0f64; SLOTS]; 5];
        for round in Round::ALL {
            let r = round.number() as usize;
            let block = 1usize << r;
            let half = block / 2;
            for s in 0..SLOTS {
                let block_start = s / block * block;
                let own_half_start = s / half * half;
                let opp_half_start = if own_half_start == block_start { block_start + half } else { block_start };
                let total: f64 = (opp_half_start..opp_half_start + half)
                    .map(|o| reach[r - 1][o] * model.series_probability(self, s, o, round, &elos))
                    .sum();
                reach[r][s] = reach[r - 1][s] * total;
            }
        }
        (0..SLOTS).map(|s| [reach[1][s], reach[2][s], reach[3][s], reach[4][s]]).collect()
    }
}

// ---------------------------------------------------------------------------------------
// Strength models
// ---------------------------------------------------------------------------------------

/// Everything a per-game model may look at.
#[derive(Debug, Clone, Copy)]
pub struct GameContext<'a> {
    pub bracket: &'a Bracket,
    pub a: usize,
    pub b: usize,
    pub a_home: bool,
    pub neutral: bool,
}

/// A model that prices single games. Series behaviour comes for free (see [`SeriesModel`]).
pub trait GameModel: Sync {
    /// P(slot `a` beats slot `b`) in one game.
    fn game_probability(&self, ctx: GameContext<'_>, elos: &[f64]) -> f64;

    /// Hook to update ratings after a simulated game. Default: ratings are fixed.
    #[inline]
    fn after_game(&self, _elos: &mut [f64], _a: usize, _b: usize, _p_a: f64, _a_won: bool) {}

    /// Whether `after_game` mutates ratings (if so, exact DP results are not available).
    fn is_stateful(&self) -> bool {
        false
    }
}

/// A model that prices whole series. Implemented for every [`GameModel`] and for models that
/// are inherently series-level (e.g. "the better seed wins with probability p").
pub trait SeriesModel: Sync {
    fn series_probability(&self, bracket: &Bracket, a: usize, b: usize, round: Round, elos: &[f64]) -> f64;

    fn simulate_series<R: Rng>(&self, bracket: &Bracket, rng: &mut R, a: usize, b: usize, round: Round, elos: &mut [f64]) -> Side;

    fn exact_available(&self) -> bool {
        true
    }
}

/// Blanket implementation: play the series game by game with the round's home pattern.
impl<M: GameModel> SeriesModel for M {
    fn series_probability(&self, bracket: &Bracket, a: usize, b: usize, round: Round, elos: &[f64]) -> f64 {
        let hc = bracket.home_court(a, b);
        let probs: Vec<f64> = bracket
            .format
            .pattern(round)
            .iter()
            .map(|&hc_hosts| {
                let a_home = if hc == a { hc_hosts } else { !hc_hosts };
                self.game_probability(GameContext { bracket, a, b, a_home, neutral: bracket.format.neutral_site }, elos)
            })
            .collect();
        elo::series_probability(&probs)
    }

    fn simulate_series<R: Rng>(&self, bracket: &Bracket, rng: &mut R, a: usize, b: usize, round: Round, elos: &mut [f64]) -> Side {
        let pattern = bracket.format.pattern(round);
        let hc = bracket.home_court(a, b);
        let mut state = SeriesState::best_of(pattern.len() as u8);
        for &hc_hosts in pattern {
            let a_home = if hc == a { hc_hosts } else { !hc_hosts };
            let p = self.game_probability(GameContext { bracket, a, b, a_home, neutral: bracket.format.neutral_site }, elos);
            let a_won = rng.gen::<f64>() < p;
            self.after_game(elos, a, b, p, a_won);
            state = state.record(if a_won { Side::A } else { Side::B });
            if let Some(w) = state.winner() {
                return w;
            }
        }
        unreachable!("a best-of-{} series resolves within {} games", pattern.len(), pattern.len())
    }

    fn exact_available(&self) -> bool {
        !self.is_stateful()
    }
}

/// Elo with home-court bonus, an optional multiplier on rating differences, and an optional
/// K-factor that updates ratings after each simulated game.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct EloModel {
    pub home_advantage: f64,
    pub scale: f64,
    pub k: Option<f64>,
}

impl Default for EloModel {
    fn default() -> Self {
        Self { home_advantage: elo::DEFAULT_HOME_ADVANTAGE, scale: 1.0, k: None }
    }
}

impl GameModel for EloModel {
    #[inline]
    fn game_probability(&self, ctx: GameContext<'_>, elos: &[f64]) -> f64 {
        elo::game_probability(elos[ctx.a], elos[ctx.b], ctx.a_home, ctx.neutral, self.home_advantage, self.scale)
    }
    #[inline]
    fn after_game(&self, elos: &mut [f64], a: usize, b: usize, p_a: f64, a_won: bool) {
        if let Some(k) = self.k {
            let (na, nb) = elo::update(elos[a], elos[b], p_a, a_won, k);
            elos[a] = na;
            elos[b] = nb;
        }
    }
    fn is_stateful(&self) -> bool {
        self.k.is_some()
    }
}

/// Every game is a coin flip.
#[derive(Debug, Clone, Copy, Default)]
pub struct EqualModel;

impl GameModel for EqualModel {
    #[inline]
    fn game_probability(&self, _: GameContext<'_>, _: &[f64]) -> f64 {
        0.5
    }
}

/// Log5 on regular-season winning percentage.
#[derive(Debug, Clone, Copy, Default)]
pub struct WinPctModel;

impl GameModel for WinPctModel {
    #[inline]
    fn game_probability(&self, ctx: GameContext<'_>, _: &[f64]) -> f64 {
        log5(ctx.bracket.teams[ctx.a].win_pct, ctx.bracket.teams[ctx.b].win_pct)
    }
}

/// The better seed (better record across conferences) wins each series with a fixed probability.
#[derive(Debug, Clone, Copy)]
pub struct SeedModel {
    pub p_better: f64,
}

impl SeriesModel for SeedModel {
    fn series_probability(&self, bracket: &Bracket, a: usize, b: usize, _round: Round, _elos: &[f64]) -> f64 {
        let (ta, tb) = (&bracket.teams[a], &bracket.teams[b]);
        let a_better = if ta.conference == tb.conference {
            ta.seed < tb.seed
        } else if ta.win_pct != tb.win_pct {
            ta.win_pct > tb.win_pct
        } else {
            bracket.home_court(a, b) == a
        };
        if a_better {
            self.p_better
        } else {
            1.0 - self.p_better
        }
    }
    fn simulate_series<R: Rng>(&self, bracket: &Bracket, rng: &mut R, a: usize, b: usize, round: Round, elos: &mut [f64]) -> Side {
        if rng.gen::<f64>() < self.series_probability(bracket, a, b, round, elos) {
            Side::A
        } else {
            Side::B
        }
    }
}

pub fn log5(pa: f64, pb: f64) -> f64 {
    let pa = pa.clamp(0.01, 0.99);
    let pb = pb.clamp(0.01, 0.99);
    (pa - pa * pb) / (pa + pb - 2.0 * pa * pb)
}

/// Serializable model configuration; converted to a concrete model type at the top of a run
/// so the inner loops are monomorphized.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "model", rename_all = "snake_case")]
pub enum ModelSpec {
    Elo {
        #[serde(default = "default_home_adv")]
        home_advantage: f64,
        #[serde(default = "default_scale")]
        scale: f64,
        #[serde(default)]
        k: Option<f64>,
    },
    Equal,
    WinPct,
    Seed {
        #[serde(default = "default_p_better")]
        p_better: f64,
    },
}

fn default_home_adv() -> f64 {
    elo::DEFAULT_HOME_ADVANTAGE
}
fn default_scale() -> f64 {
    1.0
}
fn default_p_better() -> f64 {
    0.75
}

impl Default for ModelSpec {
    fn default() -> Self {
        ModelSpec::Elo { home_advantage: elo::DEFAULT_HOME_ADVANTAGE, scale: 1.0, k: None }
    }
}

impl ModelSpec {
    pub fn name(&self) -> &'static str {
        match self {
            ModelSpec::Elo { .. } => "elo",
            ModelSpec::Equal => "equal",
            ModelSpec::WinPct => "win_pct",
            ModelSpec::Seed { .. } => "seed",
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        match self {
            ModelSpec::Elo { home_advantage, scale, k } => {
                if !(0.0..=400.0).contains(home_advantage) {
                    return Err("home advantage must be between 0 and 400 Elo points".into());
                }
                if !(0.1..=3.0).contains(scale) {
                    return Err("Elo scale must be between 0.1 and 3".into());
                }
                if let Some(k) = k {
                    if !(0.0..=100.0).contains(k) {
                        return Err("K-factor must be between 0 and 100".into());
                    }
                }
            }
            ModelSpec::Seed { p_better }
                if !(0.5..=1.0).contains(p_better) => {
                    return Err("seed model probability must be between 0.5 and 1".into());
                }
            _ => {}
        }
        Ok(())
    }

    /// Dispatch once, then run monomorphized code.
    pub fn exact(&self, bracket: &Bracket) -> Option<Vec<[f64; 4]>> {
        match *self {
            ModelSpec::Elo { home_advantage, scale, k } => {
                let m = EloModel { home_advantage, scale, k };
                m.exact_available().then(|| bracket.exact_probabilities(&m))
            }
            ModelSpec::Equal => Some(bracket.exact_probabilities(&EqualModel)),
            ModelSpec::WinPct => Some(bracket.exact_probabilities(&WinPctModel)),
            ModelSpec::Seed { p_better } => Some(bracket.exact_probabilities(&SeedModel { p_better })),
        }
    }

    pub fn simulate<P: Progress + ?Sized>(&self, bracket: &Bracket, iterations: u64, seed: u64, progress: &P) -> BracketSimulation {
        match *self {
            ModelSpec::Elo { home_advantage, scale, k } => simulate(bracket, &EloModel { home_advantage, scale, k }, iterations, seed, progress),
            ModelSpec::Equal => simulate(bracket, &EqualModel, iterations, seed, progress),
            ModelSpec::WinPct => simulate(bracket, &WinPctModel, iterations, seed, progress),
            ModelSpec::Seed { p_better } => simulate(bracket, &SeedModel { p_better }, iterations, seed, progress),
        }
    }
}

// ---------------------------------------------------------------------------------------
// Monte Carlo
// ---------------------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlotOutcome {
    pub slot: usize,
    pub team_id: i32,
    pub code: String,
    pub name: String,
    pub franchise_key: String,
    pub conference: Conference,
    pub seed: u8,
    pub elo: f64,
    pub wins: i32,
    pub losses: i32,
    pub p_second_round: f64,
    pub p_conf_finals: f64,
    pub p_finals: f64,
    pub p_champion: f64,
    pub p_champion_se: f64,
    pub exact: Option<[f64; 4]>,
    pub titles: u64,
    pub actual_result: Option<PlayoffResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BracketSimulation {
    pub iterations: u64,
    pub seed: u64,
    pub outcomes: Vec<SlotOutcome>,
    /// Frequency of each Finals matchup (East slot, West slot, count).
    pub finals_matchups: Vec<(usize, usize, u64)>,
    pub elapsed_ms: u128,
}

/// Per-slot counts of how often a slot won each round.
pub type RoundCounts = [[u64; SLOTS]; 4];

#[derive(Clone)]
struct Acc {
    iterations: u64,
    reached: RoundCounts,
    finals: [u64; 64],
}

impl Acc {
    fn new() -> Self {
        Self { iterations: 0, reached: [[0; SLOTS]; 4], finals: [0; 64] }
    }
    fn merge(mut self, o: Acc) -> Acc {
        self.iterations += o.iterations;
        for (a, b) in self.reached.iter_mut().flatten().zip(o.reached.iter().flatten()) {
            *a += b;
        }
        for (a, b) in self.finals.iter_mut().zip(&o.finals) {
            *a += b;
        }
        self
    }
}

/// Play the bracket once. Returns (champion, East finalist, West finalist) and increments
/// `reached[round][slot]` for every series winner.
pub fn play_bracket<M: SeriesModel, R: Rng>(bracket: &Bracket, rng: &mut R, model: &M, elos: &mut [f64], reached: &mut RoundCounts) -> (usize, usize, usize) {
    let mut alive: Vec<usize> = (0..SLOTS).collect();
    let mut finalists = (0usize, 0usize);
    for round in Round::ALL {
        let mut next = Vec::with_capacity(alive.len() / 2);
        for pair in alive.chunks_exact(2) {
            let (a, b) = (pair[0], pair[1]);
            let winner = match model.simulate_series(bracket, rng, a, b, round, elos) {
                Side::A => a,
                Side::B => b,
            };
            reached[round.number() as usize - 1][winner] += 1;
            next.push(winner);
        }
        if round == Round::ConfFinals {
            finalists = (next[0], next[1]);
        }
        alive = next;
    }
    (alive[0], finalists.0, finalists.1)
}

pub fn simulate<M: SeriesModel, P: Progress + ?Sized>(bracket: &Bracket, model: &M, iterations: u64, seed: u64, progress: &P) -> BracketSimulation {
    let start = std::time::Instant::now();
    let base_elos: Vec<f64> = bracket.teams.iter().map(|t| t.elo).collect();
    let chunk_list = chunks(iterations, 2_000);
    let tracker = ChunkTracker::new(progress, chunk_list.len() as u64);
    let acc = chunk_list
        .par_iter()
        .map(|&(idx, count)| {
            let mut acc = Acc::new();
            if tracker.is_cancelled() {
                return acc;
            }
            let mut rng = chunk_rng(seed, idx);
            let mut elos = base_elos.clone();
            for _ in 0..count {
                elos.copy_from_slice(&base_elos);
                let (_, e, w) = play_bracket(bracket, &mut rng, model, &mut elos, &mut acc.reached);
                acc.finals[(e % 8) * 8 + (w % 8)] += 1;
                acc.iterations += 1;
            }
            tracker.chunk_done();
            acc
        })
        .reduce(Acc::new, Acc::merge);

    let it = acc.iterations.max(1) as f64;
    let exact = model.exact_available().then(|| bracket.exact_probabilities(model));
    let outcomes = bracket
        .teams
        .iter()
        .enumerate()
        .map(|(s, t)| {
            let p_champ = acc.reached[3][s] as f64 / it;
            SlotOutcome {
                slot: s,
                team_id: t.team_id,
                code: t.code.clone(),
                name: t.name.clone(),
                franchise_key: t.franchise_key.clone(),
                conference: t.conference,
                seed: t.seed,
                elo: t.elo,
                wins: t.wins,
                losses: t.losses,
                p_second_round: acc.reached[0][s] as f64 / it,
                p_conf_finals: acc.reached[1][s] as f64 / it,
                p_finals: acc.reached[2][s] as f64 / it,
                p_champion: p_champ,
                p_champion_se: stats::binomial_se(p_champ, acc.iterations),
                exact: exact.as_ref().map(|e| e[s]),
                titles: acc.reached[3][s],
                actual_result: t.actual_result,
            }
        })
        .collect();
    let mut finals_matchups: Vec<(usize, usize, u64)> =
        acc.finals.iter().enumerate().filter(|(_, &c)| c > 0).map(|(i, &c)| (i / 8, 8 + i % 8, c)).collect();
    finals_matchups.sort_by_key(|m| std::cmp::Reverse(m.2));
    BracketSimulation { iterations: acc.iterations, seed, outcomes, finals_matchups, elapsed_ms: start.elapsed().as_millis() }
}

#[cfg(test)]
pub(crate) fn test_bracket(elos: [f64; 16]) -> Bracket {
    let teams = (0..SLOTS)
        .map(|s| {
            let seed = SLOT_SEEDS[s % 8];
            BracketTeam {
                team_id: s as i32,
                code: format!("T{s}"),
                name: format!("Team {s}"),
                franchise_key: format!("T{s}"),
                conference: conference_of(s),
                seed,
                elo: elos[s],
                wins: 82 - seed as i32 * 4,
                losses: seed as i32 * 4,
                win_pct: (82.0 - f64::from(seed) * 4.0) / 82.0,
                actual_result: None,
            }
        })
        .collect();
    Bracket { season: 2016, teams, format: PlayoffFormat::for_season(2016), actual_series: vec![], notes: vec![] }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::simulation::progress::Silent;
    use approx::assert_abs_diff_eq;

    #[test]
    fn series_state_machine() {
        let mut s = SeriesState::best_of(7);
        for _ in 0..3 {
            s = s.record(Side::A);
            assert_eq!(s.winner(), None);
        }
        s = s.record(Side::B);
        assert_eq!(s.wins(), (3, 1));
        s = s.record(Side::A);
        assert_eq!(s.winner(), Some(Side::A));
        // decided series ignore further games
        assert_eq!(s.record(Side::B), s);
        assert_eq!(SeriesState::best_of(5).record(Side::B).record(Side::B).record(Side::B).winner(), Some(Side::B));
    }

    #[test]
    fn slots_and_rounds() {
        assert_eq!(slot_for(Conference::East, 1), 0);
        assert_eq!(slot_for(Conference::East, 8), 1);
        assert_eq!(slot_for(Conference::West, 2), 12);
        assert_eq!(Bracket::meeting_round(0, 1), Some(Round::First));
        assert_eq!(Bracket::meeting_round(0, 2), Some(Round::Second));
        assert_eq!(Bracket::meeting_round(0, 7), Some(Round::ConfFinals));
        assert_eq!(Bracket::meeting_round(0, 15), Some(Round::Finals));
        assert_eq!(Bracket::meeting_round(3, 3), None);
    }

    #[test]
    fn equal_bracket_exact_is_uniform() {
        let b = test_bracket([1500.0; 16]);
        let p = b.exact_probabilities(&EqualModel);
        for probs in &p {
            assert_abs_diff_eq!(probs[3], 1.0 / 16.0, epsilon = 1e-12);
            assert_abs_diff_eq!(probs[0], 0.5, epsilon = 1e-12);
        }
    }

    #[test]
    fn exact_probabilities_sum_to_one_per_round() {
        let mut elos = [1500.0; 16];
        for (i, e) in elos.iter_mut().enumerate() {
            *e += (i as f64) * 13.0 - 50.0;
        }
        let b = test_bracket(elos);
        let p = b.exact_probabilities(&EloModel::default());
        assert_abs_diff_eq!(p.iter().map(|x| x[3]).sum::<f64>(), 1.0, epsilon = 1e-9);
        assert_abs_diff_eq!(p.iter().map(|x| x[2]).sum::<f64>(), 2.0, epsilon = 1e-9);
        assert_abs_diff_eq!(p.iter().map(|x| x[0]).sum::<f64>(), 8.0, epsilon = 1e-9);
        let best = p.iter().enumerate().max_by(|a, b| a.1[3].partial_cmp(&b.1[3]).unwrap()).unwrap().0;
        assert_eq!(best, 15);
    }

    #[test]
    fn monte_carlo_matches_exact() {
        let mut elos = [1500.0; 16];
        elos[0] = 1750.0;
        elos[8] = 1700.0;
        elos[5] = 1600.0;
        let b = test_bracket(elos);
        let sim = ModelSpec::default().simulate(&b, 100_000, 11, &Silent);
        for o in &sim.outcomes {
            let e = o.exact.unwrap();
            assert!((o.p_champion - e[3]).abs() < 4.0 * o.p_champion_se.max(0.001), "{} {} {}", o.code, o.p_champion, e[3]);
            assert!((o.p_finals - e[2]).abs() < 0.01);
        }
        assert_eq!(sim.outcomes.iter().map(|o| o.titles).sum::<u64>(), 100_000);
    }

    #[test]
    fn deterministic_with_seed() {
        let b = test_bracket([1500.0; 16]);
        let a = simulate(&b, &EloModel::default(), 5000, 3, &Silent);
        let c = simulate(&b, &EloModel::default(), 5000, 3, &Silent);
        assert_eq!(a.outcomes[0].titles, c.outcomes[0].titles);
    }

    #[test]
    fn dynamic_elo_runs_and_sums() {
        let b = test_bracket([1500.0; 16]);
        let m = EloModel { k: Some(20.0), ..Default::default() };
        assert!(!m.exact_available());
        let s = simulate(&b, &m, 2000, 1, &Silent);
        assert!(s.outcomes[0].exact.is_none());
        assert_eq!(s.outcomes.iter().map(|o| o.titles).sum::<u64>(), 2000);
    }

    #[test]
    fn bracket_advancement_is_structurally_correct() {
        let mut elos = [1500.0; 16];
        elos[3] = 9000.0;
        let b = test_bracket(elos);
        let s = simulate(&b, &EloModel { home_advantage: 0.0, scale: 1.0, k: None }, 200, 5, &Silent);
        assert_eq!(s.outcomes[3].titles, 200);
        assert_eq!(s.outcomes[2].p_second_round, 0.0);
        assert_abs_diff_eq!(s.outcomes[0].p_second_round + s.outcomes[1].p_second_round, 1.0, epsilon = 1e-12);
        for o in &s.outcomes[8..] {
            assert_eq!(o.titles, 0);
        }
        assert_abs_diff_eq!(s.outcomes[8..].iter().map(|o| o.p_finals).sum::<f64>(), 1.0, epsilon = 1e-12);
    }

    #[test]
    fn seed_model_is_series_level() {
        let b = test_bracket([1500.0; 16]);
        let p = b.exact_probabilities(&SeedModel { p_better: 1.0 });
        // the two 1-seeds meet in the Finals with identical records; East slot has home court
        assert_abs_diff_eq!(p[0][3], 1.0, epsilon = 1e-12);
        assert_abs_diff_eq!(p[8][2], 1.0, epsilon = 1e-12);
    }

    #[test]
    fn home_court_rule() {
        let b = test_bracket([1500.0; 16]);
        assert_eq!(b.home_court(0, 1), 0);
        assert_eq!(b.home_court(1, 0), 0);
        assert_eq!(b.home_court(8, 0), 0);
    }

    #[test]
    fn format_by_era() {
        assert_eq!(PlayoffFormat::for_season(2002).first_round_games, 5);
        assert_eq!(PlayoffFormat::for_season(2003).first_round_games, 7);
        assert_eq!(PlayoffFormat::for_season(2013).finals_pattern, FinalsPattern::TwoThreeTwo);
        assert_eq!(PlayoffFormat::for_season(2014).finals_pattern, FinalsPattern::TwoTwoOneOneOne);
        assert!(PlayoffFormat::for_season(2020).neutral_site);
        assert_eq!(PlayoffFormat::for_season(2002).pattern(Round::First).len(), 5);
    }
}
