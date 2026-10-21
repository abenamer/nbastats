//! Engine C: full-season Monte Carlo from an information cutoff date.
//!
//! Uses only games played on or before the cutoff. The remaining regular-season schedule is
//! simulated from Elo (optionally updating ratings game by game), standings are built with
//! the season's seeding rules, the play-in is resolved by a small state machine (actual
//! results are used when they happened on or before the cutoff), and the playoff bracket
//! is played out. Output: every team's probability of reaching the play-in, the playoffs,
//! each round, the Finals, and winning the title.

use std::collections::HashMap;

use chrono::NaiveDate;
use rand::Rng;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::domain::elo;
use crate::domain::models::{Conference, Game, GameType, PlayoffResult, Team};
use crate::domain::simulation::progress::{ChunkTracker, Progress};
use crate::domain::simulation::rng::{chunk_rng, chunks};
use crate::domain::simulation::stats;

use super::bracket::{play_bracket, Bracket, BracketTeam, EloModel, PlayoffFormat, RoundCounts, SLOTS, SLOT_SEEDS};
use super::reconstruct;
use super::seeding::{self, Qualification};
use super::standings::{order_with_tiebreaks, Standing};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SeasonParams {
    pub season: i32,
    /// Information cutoff (inclusive). `None` = before the first game of the season.
    #[serde(default)]
    pub as_of: Option<NaiveDate>,
    pub iterations: u64,
    pub seed: u64,
    #[serde(default = "default_home_adv")]
    pub home_advantage: f64,
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
    pub fn validate(&self, max_iterations: u64) -> Result<(), String> {
        if self.iterations == 0 || self.iterations > max_iterations {
            return Err(format!("iterations must be between 1 and {max_iterations}"));
        }
        if !(0.0..=400.0).contains(&self.home_advantage) {
            return Err("home advantage must be between 0 and 400 Elo points".into());
        }
        for k in [self.regular_season_k, self.playoff_k].into_iter().flatten() {
            if !(0.0..=100.0).contains(&k) {
                return Err("K-factors must be between 0 and 100".into());
            }
        }
        if !(0.1..=3.0).contains(&self.playoff_scale) {
            return Err("playoff scale must be between 0.1 and 3".into());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamForecast {
    pub team_id: i32,
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
    pub actual_result: Option<PlayoffResult>,
    pub actual_seed: Option<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SeasonForecast {
    pub season: i32,
    pub as_of: NaiveDate,
    pub information_cutoff_note: String,
    pub qualification: Qualification,
    pub games_played: usize,
    pub games_remaining: usize,
    pub iterations: u64,
    pub seed: u64,
    pub params: SeasonParams,
    pub teams: Vec<TeamForecast>,
    pub elapsed_ms: u128,
}

#[derive(Debug, Clone, thiserror::Error)]
pub enum SeasonError {
    #[error("season has no games")]
    NoGames,
    #[error("season {0} has no conference alignment data (supported from 2002-03)")]
    NoAlignment(i32),
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
    SevenVsEight { s7: Ti, s8: Ti, s9: Ti, s10: Ti },
    /// 2021+: 9 hosts 10, loser eliminated.
    NineVsTen { seed7: Ti, loser78: Ti, s9: Ti, s10: Ti },
    /// 2021+: loser of 7/8 hosts winner of 9/10 for the 8th seed.
    LoserVsWinner { seed7: Ti, loser78: Ti, winner910: Ti },
    /// 2020 bubble: 9th within four games of 8th; the 9th seed must win twice (neutral floor).
    BubbleFirst { s8: Ti, s9: Ti },
    BubbleSecond { s8: Ti, s9: Ti },
    Done { seed7: Ti, seed8: Ti },
}

impl PlayInState {
    /// (home, away, neutral) of the next game, if any.
    pub const fn pending_game(self) -> Option<(Ti, Ti, bool)> {
        match self {
            PlayInState::SevenVsEight { s7, s8, .. } => Some((s7, s8, false)),
            PlayInState::NineVsTen { s9, s10, .. } => Some((s9, s10, false)),
            PlayInState::LoserVsWinner { loser78, winner910, .. } => Some((loser78, winner910, false)),
            PlayInState::BubbleFirst { s8, s9 } | PlayInState::BubbleSecond { s8, s9 } => Some((s8, s9, true)),
            PlayInState::Done { .. } => None,
        }
    }

    pub const fn advance(self, home_won: bool) -> PlayInState {
        match self {
            PlayInState::SevenVsEight { s7, s8, s9, s10 } => {
                let (seed7, loser78) = if home_won { (s7, s8) } else { (s8, s7) };
                PlayInState::NineVsTen { seed7, loser78, s9, s10 }
            }
            PlayInState::NineVsTen { seed7, loser78, s9, s10 } => {
                PlayInState::LoserVsWinner { seed7, loser78, winner910: if home_won { s9 } else { s10 } }
            }
            PlayInState::LoserVsWinner { seed7, loser78, winner910 } => {
                PlayInState::Done { seed7, seed8: if home_won { loser78 } else { winner910 } }
            }
            PlayInState::BubbleFirst { s8, s9 } => {
                if home_won {
                    PlayInState::Done { seed7: usize::MAX, seed8: s8 }
                } else {
                    PlayInState::BubbleSecond { s8, s9 }
                }
            }
            PlayInState::BubbleSecond { s8, s9 } => PlayInState::Done { seed7: usize::MAX, seed8: if home_won { s8 } else { s9 } },
            done @ PlayInState::Done { .. } => done,
        }
    }

    /// Every team that takes part in this play-in.
    pub fn participants(self) -> impl Iterator<Item = Ti> {
        let v: [Option<Ti>; 4] = match self {
            PlayInState::SevenVsEight { s7, s8, s9, s10 } => [Some(s7), Some(s8), Some(s9), Some(s10)],
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
        }
    }
    fn merge(mut self, o: Acc) -> Acc {
        self.iterations += o.iterations;
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
        }
        self
    }
}

struct Setup {
    n: usize,
    ids: Vec<i32>,
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
}

impl Setup {
    fn build(params: &SeasonParams, games: &[Game], teams: &HashMap<i32, Team>, as_of: NaiveDate) -> Result<Setup, SeasonError> {
        let alignment = reconstruct::alignment_map(params.season, teams, games);
        if alignment.len() < 16 {
            return Err(SeasonError::NoAlignment(params.season));
        }
        let mut ids: Vec<i32> = alignment.keys().copied().collect();
        ids.sort_unstable();
        let n = ids.len();
        let idx: HashMap<i32, Ti> = ids.iter().enumerate().map(|(i, &t)| (t, i)).collect();
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
        for g in games {
            let (Some(&h), Some(&a)) = (idx.get(&g.home_team_id), idx.get(&g.away_team_id)) else { continue };
            if first_pre[h].is_nan() {
                first_pre[h] = g.home_elo_pre;
            }
            if first_pre[a].is_nan() {
                first_pre[a] = g.away_elo_pre;
            }
            let known = g.game_date <= as_of;
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
                    played_play_in.push(PlayedPlayIn { home: h, away: a, home_won: o.home_won() });
                }
                (true, Some(_), GameType::CupFinal) => {
                    elo[h] = g.home_elo_post;
                    elo[a] = g.away_elo_post;
                }
                (_, _, GameType::Regular) => remaining.push(Remaining { home: h, away: a, neutral: g.neutral }),
                _ => {}
            }
        }
        for (e, first) in elo.iter_mut().zip(&first_pre) {
            if e.is_nan() {
                *e = if first.is_nan() { 1500.0 } else { *first };
            }
        }
        let mut format = PlayoffFormat::for_season(params.season);
        format.neutral_site |= params.season == 2020;
        Ok(Setup {
            n,
            codes: ids.iter().map(|t| teams[t].code.clone()).collect(),
            names: ids.iter().map(|t| teams[t].name.clone()).collect(),
            fran: ids.iter().map(|t| teams[t].franchise_key().to_string()).collect(),
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
        })
    }

    /// Order a conference by record with tiebreaks (group head-to-head, point differential, random key).
    fn order_conference(&self, conf: Conference, st: &IterState) -> Vec<Standing> {
        let n = self.n;
        let rows: Vec<Standing> = (0..n)
            .filter(|&i| self.conf[i] == conf)
            .map(|i| Standing {
                team_id: i as i32,
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
        order_with_tiebreaks(rows, |a, b| st.h2h[a as usize * n + b as usize], |t| st.keys[t as usize])
    }

    /// Resolve a play-in game: the historical result if it was played before the cutoff,
    /// otherwise a draw from Elo.
    fn resolve_game<R: Rng>(&self, rng: &mut R, elo: &mut [f64], game: (Ti, Ti, bool), rules: GameRules) -> bool {
        let (home, away, neutral) = game;
        if let Some(p) = self.played_play_in.iter().find(|g| (g.home == home && g.away == away) || (g.home == away && g.away == home)) {
            return if p.home == home { p.home_won } else { !p.home_won };
        }
        let p = elo::game_probability(elo[home], elo[away], true, neutral, rules.home_advantage, 1.0);
        let home_won = rng.gen::<f64>() < p;
        if let Some(k) = rules.k {
            let (nh, na) = elo::update(elo[home], elo[away], p, home_won, k);
            elo[home] = nh;
            elo[away] = na;
        }
        home_won
    }

    /// The eight playoff teams of a conference in seed order, resolving the play-in.
    fn qualify<R: Rng>(&self, rng: &mut R, elo: &mut [f64], order: &[Standing], rules: GameRules, acc: &mut Acc) -> Vec<Ti> {
        let t = |i: usize| order[i].team_id as usize;
        let initial = match self.qualification {
            Qualification::Top8 => return (0..8).map(t).collect(),
            Qualification::Bubble2020 => {
                let games_behind = f64::from((order[7].wins - order[8].wins) + (order[8].losses - order[7].losses)) / 2.0;
                if games_behind <= 4.0 {
                    PlayInState::BubbleFirst { s8: t(7), s9: t(8) }
                } else {
                    return (0..8).map(t).collect();
                }
            }
            Qualification::PlayIn => PlayInState::SevenVsEight { s7: t(6), s8: t(7), s9: t(8), s10: t(9) },
        };
        for p in initial.participants() {
            acc.play_in[p] += 1;
        }
        let mut state = initial;
        while let Some(game) = state.pending_game() {
            let home_won = self.resolve_game(rng, elo, game, rules);
            state = state.advance(home_won);
        }
        let PlayInState::Done { seed7, seed8 } = state else { unreachable!("loop exits on Done") };
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
        }
    }
    fn reset<R: Rng>(&mut self, setup: &Setup, rng: &mut R) {
        self.wins.copy_from_slice(&setup.base_wins);
        self.losses.copy_from_slice(&setup.base_losses);
        self.records.copy_from_slice(&setup.base_records);
        self.h2h.copy_from_slice(&setup.base_h2h);
        self.elo.copy_from_slice(&setup.elo);
        self.keys.iter_mut().for_each(|k| *k = rng.gen());
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

pub fn run<P: Progress + ?Sized>(params: &SeasonParams, games: &[Game], teams: &HashMap<i32, Team>, progress: &P) -> Result<SeasonForecast, SeasonError> {
    let start = std::time::Instant::now();
    let first_date = games.first().map(|g| g.game_date).ok_or(SeasonError::NoGames)?;
    let first_playoff = games.iter().find(|g| g.is_playoff()).map(|g| g.game_date);
    let mut as_of = params.as_of.unwrap_or(first_date - chrono::Duration::days(1));
    let mut note = format!("Only games on or before {as_of} inform ratings and standings.");
    if let Some(fp) = first_playoff.filter(|fp| as_of >= *fp) {
        as_of = fp - chrono::Duration::days(1);
        note = format!("Cutoff moved to {as_of}, the day before the playoffs started; playoff games are never used as inputs.");
    }
    let setup = Setup::build(params, games, teams, as_of)?;
    let n = setup.n;
    let games_played = games.iter().filter(|g| g.game_date <= as_of && !g.is_playoff() && g.is_played()).count();
    let home_adv = params.home_advantage;
    let rules = GameRules { home_advantage: home_adv, k: params.regular_season_k };
    let playoff_model = EloModel { home_advantage: home_adv, scale: params.playoff_scale, k: params.playoff_k };
    let chunk_list = chunks(params.iterations, 250);
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
            for _ in 0..count {
                st.reset(&setup, &mut rng);
                // remaining regular season
                for g in &setup.remaining {
                    let p = elo::game_probability(st.elo[g.home], st.elo[g.away], true, g.neutral, home_adv, 1.0);
                    let home_won = rng.gen::<f64>() < p;
                    if let Some(k) = params.regular_season_k {
                        let (nh, na) = elo::update(st.elo[g.home], st.elo[g.away], p, home_won, k);
                        st.elo[g.home] = nh;
                        st.elo[g.away] = na;
                    }
                    let (w, l) = if home_won { (g.home, g.away) } else { (g.away, g.home) };
                    st.record(&setup, w, l);
                }
                for i in 0..n {
                    acc.wins_sum[i] += f64::from(st.wins[i]);
                }
                // standings, seeding and qualification per conference
                let mut bracket_teams: Vec<BracketTeam> = Vec::with_capacity(SLOTS);
                let mut ok = true;
                for conf in [Conference::East, Conference::West] {
                    let ordered = setup.order_conference(conf, &st);
                    let seeded = seeding::seed_order(params.season, &ordered);
                    for (pos, s) in seeded.iter().enumerate().take(16) {
                        let i = s.team_id as usize;
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
                    let mut conf_slots: [Option<BracketTeam>; 8] = Default::default();
                    for (seed_idx, &ti) in qualified.iter().enumerate() {
                        acc.playoffs[ti] += 1;
                        let seed = (seed_idx + 1) as u8;
                        let p = SLOT_SEEDS.iter().position(|&s| s == seed).expect("seed 1-8");
                        conf_slots[p] = Some(BracketTeam {
                            team_id: ti as i32,
                            code: String::new(),
                            name: String::new(),
                            franchise_key: String::new(),
                            conference: conf,
                            seed,
                            elo: st.elo[ti],
                            wins: st.wins[ti],
                            losses: st.losses[ti],
                            win_pct: f64::from(st.wins[ti]) / f64::from((st.wins[ti] + st.losses[ti]).max(1)),
                            actual_result: None,
                        });
                    }
                    bracket_teams.extend(conf_slots.into_iter().map(|s| s.expect("eight qualifiers")));
                }
                if !ok {
                    continue;
                }
                let bracket = Bracket { season: params.season, teams: bracket_teams, format: setup.format, actual_series: vec![], notes: vec![] };
                let mut b_elos: Vec<f64> = bracket.teams.iter().map(|t| t.elo).collect();
                reached.iter_mut().flatten().for_each(|x| *x = 0);
                play_bracket(&bracket, &mut rng, &playoff_model, &mut b_elos, &mut reached);
                for (slot, bt) in bracket.teams.iter().enumerate() {
                    let ti = bt.team_id as usize;
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
    let actual_seed: HashMap<i32, u8> = reconstruct::reconstruct(params.season, games, teams)
        .map(|r| r.bracket.teams.iter().map(|t| (t.team_id, t.seed)).collect())
        .unwrap_or_default();
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
                remaining_games: setup.remaining.iter().filter(|g| g.home == i || g.away == i).count() as i32,
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
                actual_result: actual_results.get(&setup.ids[i]).copied(),
                actual_seed: actual_seed.get(&setup.ids[i]).copied(),
            }
        })
        .collect();
    teams_out.sort_by(|a, b| b.p_champion.partial_cmp(&a.p_champion).unwrap_or(std::cmp::Ordering::Equal).then_with(|| a.code.cmp(&b.code)));
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
        elapsed_ms: start.elapsed().as_millis(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn play_in_tournament_state_machine() {
        let s = PlayInState::SevenVsEight { s7: 7, s8: 8, s9: 9, s10: 10 };
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
        assert_eq!(s.advance(true), PlayInState::Done { seed7: usize::MAX, seed8: 1 });
        let s = s.advance(false);
        assert_eq!(s, PlayInState::BubbleSecond { s8: 1, s9: 2 });
        assert_eq!(s.pending_game(), Some((1, 2, true)));
        assert_eq!(s.advance(false), PlayInState::Done { seed7: usize::MAX, seed8: 2 });
    }
}
