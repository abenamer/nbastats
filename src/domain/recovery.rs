//! Return from absence: what a player delivers after a run of missed games, read from the
//! stored box scores, against what he delivered before.
//!
//! The game logs carry no injury reports before the present, but an injury leaves a
//! footprint: a regular player disappears from his team's lines for a run of games and
//! comes back. Every such run is a return event. The module measures the player's
//! box-score worth per game played, his minutes share and his availability in the twenty
//! team games before the run and in the twenty after the return (in two halves), groups the
//! events by the player's Box Plus/Minus before the run and by the run's length, and sets
//! them against a control: every stretch of forty team games of a regular player with no
//! such run, read in the same windows and grouped by the same bracket rule, so the drop a
//! healthy player shows between two windows (regression to the mean, strongest where the
//! first window was good) can be taken off the drop after a return.
//!
//! This is a report, not a forecast rule: nothing here changes a rating or a penalty.

use std::collections::{BTreeMap, HashMap};

use chrono::NaiveDate;
use serde::{Serialize, Serializer};

use crate::domain::bpm::{self, BoxLine, BoxTable, Per100, REPLACEMENT_LEVEL};
use crate::domain::ids::{PlayerId, SeasonYear, TeamId};
use crate::domain::models::{Game, GameType};
use crate::domain::worth::ELO_PER_POINT;

/// Named in the report, so a reader knows which rules produced it.
pub const RETURN_MODEL_VERSION: &str = "return-curve-1.0";
/// The fewest team games missed in a row that count as a return event.
pub const MIN_SPELL: u32 = 5;
/// Team games in the window before the run and after the return.
pub const WINDOW: usize = 20;
/// The post-return window is read in two halves of this many team games.
pub const HALF: usize = 10;
/// A regular player: games played in the window before the run.
pub const MIN_PRE_PLAYED: u32 = 15;
/// A regular player: minutes per game played in the window before the run.
pub const MIN_PRE_MINUTES: f64 = 15.0;
/// Bracket edges on the player's BPM in the window before the run.
pub const BRACKET_EDGES: [f64; 3] = [0.0, 2.0, 5.0];
/// Bucket edges on the team games missed.
pub const LENGTH_EDGES: [u32; 3] = [10, 20, 40];

/// A player's quality before the run, by his BPM in the window before it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Bracket {
    BelowZero,
    ZeroToTwo,
    TwoToFive,
    FiveAndUp,
}

impl Bracket {
    pub const ALL: [Bracket; 4] = [
        Bracket::BelowZero,
        Bracket::ZeroToTwo,
        Bracket::TwoToFive,
        Bracket::FiveAndUp,
    ];

    pub fn of(bpm: f64) -> Bracket {
        if bpm < BRACKET_EDGES[0] {
            Bracket::BelowZero
        } else if bpm < BRACKET_EDGES[1] {
            Bracket::ZeroToTwo
        } else if bpm < BRACKET_EDGES[2] {
            Bracket::TwoToFive
        } else {
            Bracket::FiveAndUp
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Bracket::BelowZero => "BPM below 0",
            Bracket::ZeroToTwo => "BPM 0 to 2",
            Bracket::TwoToFive => "BPM 2 to 5",
            Bracket::FiveAndUp => "BPM 5 and up",
        }
    }
}

impl Serialize for Bracket {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.label())
    }
}

/// How long the run was, in team games missed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Length {
    FiveToNine,
    TenToNineteen,
    TwentyToThirtyNine,
    FortyAndUp,
}

impl Length {
    pub const ALL: [Length; 4] = [
        Length::FiveToNine,
        Length::TenToNineteen,
        Length::TwentyToThirtyNine,
        Length::FortyAndUp,
    ];

    pub fn of(missed: u32) -> Length {
        if missed < LENGTH_EDGES[0] {
            Length::FiveToNine
        } else if missed < LENGTH_EDGES[1] {
            Length::TenToNineteen
        } else if missed < LENGTH_EDGES[2] {
            Length::TwentyToThirtyNine
        } else {
            Length::FortyAndUp
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Length::FiveToNine => "5 to 9 games",
            Length::TenToNineteen => "10 to 19 games",
            Length::TwentyToThirtyNine => "20 to 39 games",
            Length::FortyAndUp => "40 games and more",
        }
    }
}

impl Serialize for Length {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.label())
    }
}

/// What a player delivered over a window of his team's games.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Window {
    /// Team games in the window.
    pub team_games: u32,
    /// Games he played (minutes above zero).
    pub played: u32,
    /// `played` over `team_games`.
    pub availability: f64,
    /// Minutes per game played; zero when he played none.
    pub minutes_per_game: f64,
    /// His share of the floor in the games he played; none when he played none.
    pub share: Option<f64>,
    /// Box Plus/Minus over the window, with the season's position, role and team
    /// adjustment; none when he played none.
    pub bpm: Option<f64>,
    /// Box worth per game played, in Elo; none when he played none.
    pub worth_elo: Option<f64>,
}

/// A regular player over forty team games with no run of missed games, away from any run
/// of his: the drop between its windows is what two windows show without an injury. The
/// stretches of one player start every ten games, so they overlap.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ControlSample {
    pub season: SeasonYear,
    pub team_id: TeamId,
    pub player_id: PlayerId,
    /// By the BPM of the first twenty games, as an event is by the window before its run.
    pub bracket: Bracket,
    pub pre: Window,
    pub early: Window,
    pub late: Window,
}

/// One run of missed games with a return, measured before and after.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ReturnEvent {
    pub season: SeasonYear,
    pub team_id: TeamId,
    pub player_id: PlayerId,
    /// Team games missed in a row.
    pub missed: u32,
    /// The date of the first game missed.
    pub first_missed: NaiveDate,
    /// The date of the game he came back in.
    pub returned: NaiveDate,
    pub bracket: Bracket,
    pub length: Length,
    /// The twenty team games before the run.
    pub pre: Window,
    /// The first ten team games from the return.
    pub early: Window,
    /// The next ten team games, when the season had any.
    pub late: Option<Window>,
}

impl ReturnEvent {
    /// Worth per game played after the return minus before, early half.
    pub fn drop_early(&self) -> Option<f64> {
        Some(self.early.worth_elo? - self.pre.worth_elo?)
    }

    /// The same for the late half.
    pub fn drop_late(&self) -> Option<f64> {
        Some(self.late?.worth_elo? - self.pre.worth_elo?)
    }
}

impl ControlSample {
    pub fn drop_early(&self) -> Option<f64> {
        Some(self.early.worth_elo? - self.pre.worth_elo?)
    }

    pub fn drop_late(&self) -> Option<f64> {
        Some(self.late.worth_elo? - self.pre.worth_elo?)
    }
}

/// The events of one season.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SeasonReturns {
    pub season: SeasonYear,
    /// Team games with lines, summed over the teams.
    pub team_games: u32,
    /// Player seasons with a team that were walked for runs.
    pub player_teams: u32,
    pub events: Vec<ReturnEvent>,
    pub controls: Vec<ControlSample>,
    /// Why the season has no events when it has none.
    pub note: Option<String>,
}

/// The means of one group of events: a bracket, a length, both or neither (the total).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Cell {
    pub bracket: Option<Bracket>,
    pub length: Option<Length>,
    pub events: u32,
    /// Events with a late window in which the player played.
    pub late_events: u32,
    pub pre_worth: Option<f64>,
    pub early_worth: Option<f64>,
    pub late_worth: Option<f64>,
    pub pre_bpm: Option<f64>,
    pub early_bpm: Option<f64>,
    pub late_bpm: Option<f64>,
    pub pre_share: Option<f64>,
    pub early_share: Option<f64>,
    pub late_share: Option<f64>,
    pub pre_minutes: Option<f64>,
    pub early_minutes: Option<f64>,
    pub late_minutes: Option<f64>,
    pub early_availability: Option<f64>,
    pub late_availability: Option<f64>,
    /// Mean of (early worth minus pre worth) over the events, in Elo per game played.
    pub drop_early: Option<f64>,
    pub drop_early_se: Option<f64>,
    pub drop_late: Option<f64>,
    /// Control samples in the brackets the cell's events fall in.
    pub controls: u32,
    /// The controls' drop, each event taking its own bracket's mean: what two windows show
    /// without an injury for players like these.
    pub control_drop_early: Option<f64>,
    pub control_drop_late: Option<f64>,
    /// Mean over the events of (event drop minus its bracket's control drop): the drop a
    /// return adds to what two windows show anyway. Its error combines the events' and the
    /// controls'. None when a bracket of the cell has fewer than two control samples.
    pub effect_early: Option<f64>,
    pub effect_early_se: Option<f64>,
    pub effect_late: Option<f64>,
}

/// The rules the report was made with, for the reader.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Parameters {
    pub model: &'static str,
    pub min_spell: u32,
    pub window: usize,
    pub half: usize,
    pub min_pre_played: u32,
    pub min_pre_minutes: f64,
    pub bracket_edges: [f64; 3],
    pub length_edges: [u32; 3],
    pub brackets: Vec<&'static str>,
    pub lengths: Vec<&'static str>,
    pub elo_per_point: f64,
    pub replacement_level: f64,
}

pub fn parameters() -> Parameters {
    Parameters {
        model: RETURN_MODEL_VERSION,
        min_spell: MIN_SPELL,
        window: WINDOW,
        half: HALF,
        min_pre_played: MIN_PRE_PLAYED,
        min_pre_minutes: MIN_PRE_MINUTES,
        bracket_edges: BRACKET_EDGES,
        length_edges: LENGTH_EDGES,
        brackets: Bracket::ALL.iter().map(|b| b.label()).collect(),
        lengths: Length::ALL.iter().map(|l| l.label()).collect(),
        elo_per_point: ELO_PER_POINT,
        replacement_level: REPLACEMENT_LEVEL,
    }
}

/// The season-level figures a window's rates lean on.
struct Context {
    season: SeasonYear,
    position: f64,
    role: f64,
    team_adjustment: f64,
    possessions_per_game: f64,
    team_pts_per_tsa: f64,
    league_pts_per_tsa: f64,
    turnovers_covered: bool,
}

#[derive(Default)]
struct Sum {
    seconds: f64,
    pts: f64,
    fg3m: f64,
    ast: f64,
    tov: f64,
    oreb: f64,
    dreb: f64,
    reb: f64,
    stl: f64,
    blk: f64,
    pf: f64,
    fga: f64,
    fta: f64,
}

impl Sum {
    fn add(&mut self, l: &BoxLine) {
        let f = |v: Option<i32>| f64::from(v.unwrap_or(0));
        self.seconds += f(l.seconds);
        self.pts += f(l.pts);
        self.fg3m += f(l.fg3m);
        self.ast += f(l.ast);
        self.tov += f(l.tov);
        self.oreb += f(l.oreb);
        self.dreb += f(l.dreb);
        self.reb += f(l.reb.or_else(|| match (l.oreb, l.dreb) {
            (Some(o), Some(d)) => Some(o + d),
            _ => None,
        }));
        self.stl += f(l.stl);
        self.blk += f(l.blk);
        self.pf += f(l.pf);
        self.fga += f(l.fga);
        self.fta += f(l.fta);
    }

    fn tsa(&self) -> f64 {
        self.fga + bpm::FTA_WEIGHT * self.fta
    }
}

/// One player's view of his team's games: his line in each, and the team's minutes.
struct Sequence<'a> {
    lines: Vec<Option<&'a BoxLine>>,
    played: Vec<bool>,
    team_seconds: &'a [f64],
}

impl Sequence<'_> {
    fn len(&self) -> usize {
        self.played.len()
    }

    /// The player over the team games `range`.
    fn measure(&self, range: std::ops::Range<usize>, ctx: &Context) -> Window {
        let team_games = range.len() as u32;
        let mut sum = Sum::default();
        let mut team_seconds = 0.0;
        let mut played = 0u32;
        for i in range {
            if let (Some(l), true) = (self.lines[i], self.played[i]) {
                sum.add(l);
                team_seconds += self.team_seconds[i];
                played += 1;
            }
        }
        let availability = if team_games > 0 {
            f64::from(played) / f64::from(team_games)
        } else {
            0.0
        };
        if played == 0 || team_seconds <= 0.0 || sum.seconds <= 0.0 {
            return Window {
                team_games,
                played,
                availability,
                minutes_per_game: 0.0,
                share: None,
                bpm: None,
                worth_elo: None,
            };
        }
        let share = sum.seconds / (team_seconds / 5.0);
        let on_floor = ctx.possessions_per_game * f64::from(played) * share;
        let per100 = |v: f64| {
            if on_floor > 0.0 {
                v / on_floor * 100.0
            } else {
                0.0
            }
        };
        let tov = if ctx.turnovers_covered {
            per100(sum.tov)
        } else {
            bpm::imputed_turnovers(
                ctx.season,
                per100(sum.pts),
                per100(sum.reb),
                per100(sum.fga),
                per100(sum.fta),
                per100(sum.ast),
                per100(sum.pf),
            )
        };
        let rates = Per100 {
            adj_pts: per100(sum.pts)
                - per100(sum.tsa()) * (ctx.team_pts_per_tsa - ctx.league_pts_per_tsa),
            fg3m: per100(sum.fg3m),
            ast: per100(sum.ast),
            tov,
            orb: per100(sum.oreb),
            drb: per100(sum.dreb),
            stl: per100(sum.stl),
            blk: per100(sum.blk),
            pf: per100(sum.pf),
            fga: per100(sum.fga),
            fta: per100(sum.fta),
        };
        let value = bpm::raw_bpm(&rates, ctx.position, ctx.role) + ctx.team_adjustment;
        let worth =
            (value - REPLACEMENT_LEVEL) * share * ctx.possessions_per_game / 100.0 * ELO_PER_POINT;
        Window {
            team_games,
            played,
            availability,
            minutes_per_game: sum.seconds / 60.0 / f64::from(played),
            share: Some(share),
            bpm: Some(value),
            worth_elo: Some(worth),
        }
    }

    /// The length of the run of missed games each index lies in (zero for a game played).
    fn run_lengths(&self) -> Vec<u32> {
        let n = self.len();
        let mut out = vec![0u32; n];
        let mut i = 0;
        while i < n {
            if self.played[i] {
                i += 1;
                continue;
            }
            let start = i;
            while i < n && !self.played[i] {
                i += 1;
            }
            let len = (i - start) as u32;
            for slot in &mut out[start..i] {
                *slot = len;
            }
        }
        out
    }

    /// A regular player over the window starting at `s`: enough games and minutes.
    fn regular(&self, s: usize, ctx: &Context) -> Option<Window> {
        let w = self.measure(s..s + WINDOW, ctx);
        (w.played >= MIN_PRE_PLAYED && w.minutes_per_game >= MIN_PRE_MINUTES).then_some(w)
    }

    /// The control samples of the player: every stretch of forty team games starting at a
    /// multiple of ten, with no run of `MIN_SPELL` inside, a regular player in its first
    /// twenty, and no overlap with the windows `blocked` around his runs.
    fn controls(
        &self,
        blocked: &[(usize, usize)],
        runs: &[u32],
        ctx: &Context,
        key: (SeasonYear, TeamId, PlayerId),
    ) -> Vec<ControlSample> {
        let n = self.len();
        let span = 2 * WINDOW;
        let mut out = Vec::new();
        if n < span {
            return out;
        }
        for s in (0..=n - span).step_by(HALF) {
            let e = s + span;
            if blocked.iter().any(|(lo, hi)| s < *hi && e > *lo) {
                continue;
            }
            if runs[s..e].iter().any(|r| *r >= MIN_SPELL) {
                continue;
            }
            let Some(pre) = self.regular(s, ctx) else {
                continue;
            };
            let Some(bpm) = pre.bpm else {
                continue;
            };
            out.push(ControlSample {
                season: key.0,
                team_id: key.1,
                player_id: key.2,
                bracket: Bracket::of(bpm),
                pre,
                early: self.measure(s + WINDOW..s + WINDOW + HALF, ctx),
                late: self.measure(s + WINDOW + HALF..e, ctx),
            });
        }
        out
    }
}

/// The return events of a season, from its regular-season games with box scores and the
/// season's box-score table (for each player's position and role and each team's pace
/// and adjustment).
pub fn season_returns(
    season: SeasonYear,
    games: &[Game],
    lines: &[BoxLine],
    table: &BoxTable,
) -> SeasonReturns {
    let empty = |note: String| SeasonReturns {
        season,
        team_games: 0,
        player_teams: 0,
        events: vec![],
        controls: vec![],
        note: Some(note),
    };
    if !table.bpm_available {
        return empty(table.note.clone().unwrap_or_else(|| {
            "The season's box scores do not support Box Plus/Minus.".to_string()
        }));
    }
    let mut ordered: Vec<&Game> = games
        .iter()
        .filter(|g| g.game_type == GameType::Regular && g.is_played())
        .collect();
    ordered.sort_by_key(|g| (g.game_date, g.seq, g.id));
    let order: HashMap<i64, usize> = ordered.iter().enumerate().map(|(i, g)| (g.id, i)).collect();
    // Each team's lines per game, in schedule order.
    let mut by_team: BTreeMap<TeamId, BTreeMap<usize, Vec<&BoxLine>>> = BTreeMap::new();
    for l in lines {
        if let Some(&i) = order.get(&l.game_id) {
            by_team
                .entry(l.team_id)
                .or_default()
                .entry(i)
                .or_default()
                .push(l);
        }
    }
    let players: HashMap<(TeamId, PlayerId), (f64, f64)> = table
        .players
        .iter()
        .map(|p| ((p.team_id, p.player_id), (p.position, p.offensive_role)))
        .collect();
    let mut events = Vec::new();
    let mut controls = Vec::new();
    let mut team_games = 0u32;
    let mut player_teams = 0u32;
    for (team, per_game) in &by_team {
        let Some(team_row) = table.teams.iter().find(|t| t.team_id == *team) else {
            continue;
        };
        if team_row.games == 0 {
            continue;
        }
        let game_lines: Vec<&Vec<&BoxLine>> = per_game.values().collect();
        let dates: Vec<NaiveDate> = per_game.keys().map(|i| ordered[*i].game_date).collect();
        let n = game_lines.len();
        team_games += n as u32;
        let team_seconds: Vec<f64> = game_lines
            .iter()
            .map(|ls| {
                ls.iter()
                    .map(|l| f64::from(l.seconds.unwrap_or(0)))
                    .sum::<f64>()
            })
            .collect();
        let mut ids: Vec<PlayerId> = game_lines
            .iter()
            .flat_map(|ls| ls.iter())
            .filter(|l| l.seconds.unwrap_or(0) > 0)
            .map(|l| l.player_id)
            .collect();
        ids.sort_unstable();
        ids.dedup();
        for player in ids {
            let Some(&(position, role)) = players.get(&(*team, player)) else {
                continue;
            };
            player_teams += 1;
            let ctx = Context {
                season,
                position,
                role,
                team_adjustment: team_row.team_adjustment,
                possessions_per_game: team_row.possessions / f64::from(team_row.games),
                team_pts_per_tsa: team_row.pts_per_tsa,
                league_pts_per_tsa: table.league.pts_per_tsa,
                turnovers_covered: table.coverage.turnovers,
            };
            let seq_lines: Vec<Option<&BoxLine>> = game_lines
                .iter()
                .map(|ls| ls.iter().copied().find(|l| l.player_id == player))
                .collect();
            let played: Vec<bool> = seq_lines
                .iter()
                .map(|l| l.is_some_and(|l| l.seconds.unwrap_or(0) > 0))
                .collect();
            let seq = Sequence {
                lines: seq_lines,
                played,
                team_seconds: &team_seconds,
            };
            let runs = seq.run_lengths();
            // The windows around every run of five, whether or not it is an event: a
            // control stretch keeps clear of them.
            let mut blocked: Vec<(usize, usize)> = Vec::new();
            let mut i = 0;
            while i < n {
                if seq.played[i] {
                    i += 1;
                    continue;
                }
                let start = i;
                while i < n && !seq.played[i] {
                    i += 1;
                }
                let end = i;
                let missed = (end - start) as u32;
                if missed >= MIN_SPELL {
                    blocked.push((start.saturating_sub(WINDOW), (end + WINDOW).min(n)));
                }
                if end == n {
                    break;
                }
                if missed < MIN_SPELL || start < WINDOW || n - end < HALF {
                    continue;
                }
                // The window before must be clean, as a control stretch is: a run inside
                // it would make the "before" a return of its own.
                if runs[start - WINDOW..start].iter().any(|r| *r >= MIN_SPELL) {
                    continue;
                }
                let Some(pre) = seq.regular(start - WINDOW, &ctx) else {
                    continue;
                };
                let Some(pre_bpm) = pre.bpm else {
                    continue;
                };
                let early = seq.measure(end..end + HALF, &ctx);
                let late =
                    (end + HALF < n).then(|| seq.measure(end + HALF..(end + WINDOW).min(n), &ctx));
                events.push(ReturnEvent {
                    season,
                    team_id: *team,
                    player_id: player,
                    missed,
                    first_missed: dates[start],
                    returned: dates[end],
                    bracket: Bracket::of(pre_bpm),
                    length: Length::of(missed),
                    pre,
                    early,
                    late,
                });
            }
            controls.extend(seq.controls(&blocked, &runs, &ctx, (season, *team, player)));
        }
    }
    events.sort_by(|a, b| {
        a.first_missed
            .cmp(&b.first_missed)
            .then(a.team_id.cmp(&b.team_id))
            .then(a.player_id.cmp(&b.player_id))
    });
    let note = if events.is_empty() {
        Some(if team_games == 0 {
            "No regular-season game of this season has box-score lines.".to_string()
        } else {
            "No regular player missed five team games in a row and came back with twenty games before and ten after.".to_string()
        })
    } else {
        None
    };
    SeasonReturns {
        season,
        team_games,
        player_teams,
        events,
        controls,
        note,
    }
}

fn mean(values: impl Iterator<Item = f64>) -> Option<f64> {
    let (mut n, mut sum) = (0usize, 0.0);
    for v in values {
        n += 1;
        sum += v;
    }
    (n > 0).then(|| sum / n as f64)
}

/// Mean and its standard error (none below two values).
fn mean_se(values: &[f64]) -> (Option<f64>, Option<f64>) {
    let n = values.len();
    if n == 0 {
        return (None, None);
    }
    let m = values.iter().sum::<f64>() / n as f64;
    if n < 2 {
        return (Some(m), None);
    }
    let var = values.iter().map(|v| (v - m).powi(2)).sum::<f64>() / (n - 1) as f64;
    (Some(m), Some((var / n as f64).sqrt()))
}

/// The controls of one bracket: how many, and the mean drop with its error, early and late.
struct BracketControl {
    samples: u32,
    early: Option<f64>,
    early_se: Option<f64>,
    late: Option<f64>,
}

fn bracket_controls(controls: &[ControlSample]) -> HashMap<Bracket, BracketControl> {
    Bracket::ALL
        .iter()
        .map(|b| {
            let picked: Vec<&ControlSample> = controls.iter().filter(|c| c.bracket == *b).collect();
            let drops: Vec<f64> = picked.iter().filter_map(|c| c.drop_early()).collect();
            let (early, early_se) = mean_se(&drops);
            (
                *b,
                BracketControl {
                    samples: picked.len() as u32,
                    early,
                    early_se,
                    late: mean(picked.iter().filter_map(|c| c.drop_late())),
                },
            )
        })
        .collect()
}

fn cell(
    bracket: Option<Bracket>,
    length: Option<Length>,
    events: &[&ReturnEvent],
    controls: &HashMap<Bracket, BracketControl>,
) -> Cell {
    let late: Vec<&Window> = events
        .iter()
        .filter_map(|e| e.late.as_ref())
        .filter(|w| w.played > 0)
        .collect();
    let drops_early: Vec<f64> = events.iter().filter_map(|e| e.drop_early()).collect();
    let (drop_early, drop_early_se) = mean_se(&drops_early);
    // The brackets the events fall in, with how many events each.
    let mut present: BTreeMap<Bracket, u32> = BTreeMap::new();
    for e in events {
        *present.entry(e.bracket).or_default() += 1;
    }
    let control_of = |b: Bracket| controls.get(&b);
    let samples: u32 = present
        .keys()
        .filter_map(|b| control_of(*b).map(|c| c.samples))
        .sum();
    // Every bracket present needs at least two control samples, so the control's own
    // error is known; the late half needs their late windows too.
    let every_bracket_controlled = present
        .keys()
        .all(|b| control_of(*b).is_some_and(|c| c.early_se.is_some()));
    let late_controlled = present
        .keys()
        .all(|b| control_of(*b).is_some_and(|c| c.early_se.is_some() && c.late.is_some()));
    let effects_early: Vec<f64> = events
        .iter()
        .filter_map(|e| Some(e.drop_early()? - control_of(e.bracket)?.early?))
        .collect();
    let (mut effect_early, mut effect_early_se) = mean_se(&effects_early);
    if !every_bracket_controlled || effects_early.is_empty() {
        effect_early = None;
        effect_early_se = None;
    } else if let Some(se) = effect_early_se {
        // The controls' error, each bracket weighted by its share of the events.
        let n = effects_early.len() as f64;
        let control_var: f64 = present
            .iter()
            .map(|(b, k)| {
                let w = f64::from(*k) / n;
                control_of(*b)
                    .and_then(|c| c.early_se)
                    .map_or(0.0, |s| (w * s).powi(2))
            })
            .sum();
        effect_early_se = Some((se * se + control_var).sqrt());
    }
    let effect_late = late_controlled
        .then(|| {
            mean(
                events
                    .iter()
                    .filter_map(|e| Some(e.drop_late()? - control_of(e.bracket)?.late?)),
            )
        })
        .flatten();
    Cell {
        bracket,
        length,
        events: events.len() as u32,
        late_events: late.len() as u32,
        pre_worth: mean(events.iter().filter_map(|e| e.pre.worth_elo)),
        early_worth: mean(events.iter().filter_map(|e| e.early.worth_elo)),
        late_worth: mean(late.iter().filter_map(|w| w.worth_elo)),
        pre_bpm: mean(events.iter().filter_map(|e| e.pre.bpm)),
        early_bpm: mean(events.iter().filter_map(|e| e.early.bpm)),
        late_bpm: mean(late.iter().filter_map(|w| w.bpm)),
        pre_share: mean(events.iter().filter_map(|e| e.pre.share)),
        early_share: mean(events.iter().filter_map(|e| e.early.share)),
        late_share: mean(late.iter().filter_map(|w| w.share)),
        pre_minutes: mean(events.iter().map(|e| e.pre.minutes_per_game)),
        early_minutes: mean(
            events
                .iter()
                .filter(|e| e.early.played > 0)
                .map(|e| e.early.minutes_per_game),
        ),
        late_minutes: mean(late.iter().map(|w| w.minutes_per_game)),
        early_availability: mean(events.iter().map(|e| e.early.availability)),
        late_availability: mean(
            events
                .iter()
                .filter_map(|e| e.late.as_ref())
                .map(|w| w.availability),
        ),
        drop_early,
        drop_early_se,
        drop_late: mean(events.iter().filter_map(|e| e.drop_late())),
        controls: samples,
        control_drop_early: every_bracket_controlled
            .then(|| mean(events.iter().filter_map(|e| control_of(e.bracket)?.early)))
            .flatten(),
        control_drop_late: late_controlled
            .then(|| mean(events.iter().filter_map(|e| control_of(e.bracket)?.late)))
            .flatten(),
        effect_early,
        effect_early_se,
        effect_late,
    }
}

/// The cells of the report: the total, each bracket, each length and each bracket by
/// length, in that order; a cell with no event is left out except the total.
pub fn cells(events: &[ReturnEvent], controls: &[ControlSample]) -> Vec<Cell> {
    let by_bracket = bracket_controls(controls);
    let mut out = Vec::new();
    let all: Vec<&ReturnEvent> = events.iter().collect();
    out.push(cell(None, None, &all, &by_bracket));
    for b in Bracket::ALL {
        let picked: Vec<&ReturnEvent> = events.iter().filter(|e| e.bracket == b).collect();
        if !picked.is_empty() {
            out.push(cell(Some(b), None, &picked, &by_bracket));
        }
    }
    for l in Length::ALL {
        let picked: Vec<&ReturnEvent> = events.iter().filter(|e| e.length == l).collect();
        if !picked.is_empty() {
            out.push(cell(None, Some(l), &picked, &by_bracket));
        }
    }
    for b in Bracket::ALL {
        for l in Length::ALL {
            let picked: Vec<&ReturnEvent> = events
                .iter()
                .filter(|e| e.bracket == b && e.length == l)
                .collect();
            if !picked.is_empty() {
                out.push(cell(Some(b), Some(l), &picked, &by_bracket));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::models::{EloSource, ResultSource};

    const S: SeasonYear = SeasonYear::of(2016);
    const ROUNDS: usize = 100;

    /// Slot profiles: minutes, pts, oreb, dreb, ast, stl, blk, tov, pf, fgm, fga, fg3m, ftm, fta.
    const SLOTS: [[i32; 14]; 8] = [
        [36, 26, 2, 6, 7, 2, 1, 3, 2, 9, 19, 3, 5, 6],
        [34, 20, 1, 5, 4, 1, 0, 2, 2, 7, 15, 2, 4, 5],
        [32, 16, 1, 4, 3, 1, 1, 2, 3, 6, 12, 1, 3, 4],
        [30, 14, 3, 7, 2, 1, 2, 1, 3, 5, 10, 0, 4, 5],
        [28, 10, 1, 3, 2, 1, 0, 1, 2, 4, 8, 1, 1, 2],
        [26, 8, 1, 2, 1, 0, 0, 1, 2, 3, 7, 1, 1, 1],
        [24, 6, 0, 2, 1, 0, 0, 1, 2, 2, 5, 0, 2, 2],
        [30, 4, 1, 3, 1, 0, 0, 0, 2, 2, 4, 0, 0, 1],
    ];

    fn game(id: i64, day: i64, home: i32, away: i32, hs: i32, as_: i32) -> Game {
        Game {
            id,
            season: S,
            game_date: NaiveDate::from_ymd_opt(2015, 11, 1).unwrap() + chrono::Duration::days(day),
            seq: 0,
            home_team_id: TeamId::new(home),
            away_team_id: TeamId::new(away),
            neutral: false,
            game_type: GameType::Regular,
            home_score: Some(hs),
            away_score: Some(as_),
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

    /// The `k`-th game of team `t` in the round-robin: which players sit and how many
    /// minutes the star plays (none means his profile).
    fn roster_rule(team: i32, k: usize) -> (Vec<usize>, Option<i32>) {
        match team {
            // The star misses games 25..37 and plays 24 minutes in his first ten back.
            1 if (25..37).contains(&k) => (vec![0], None),
            1 if (37..47).contains(&k) => (vec![], Some(24)),
            // A three-game gap: no event. A six-game run late in the season: an event whose
            // control lies before it and whose late window is short.
            2 if (30..33).contains(&k) => (vec![1], None),
            2 if (80..86).contains(&k) => (vec![1], None),
            // Out from game 70 to the end: no return.
            3 if k >= 70 => (vec![0], None),
            // Five out, back for seven, eight out: the first run is an event, the second
            // is not, since its window before holds the first.
            3 if (50..55).contains(&k) || (62..70).contains(&k) => (vec![4], None),
            // Out too early for a window before, then out seven games with a return.
            4 if (10..20).contains(&k) => (vec![7], None),
            4 if (40..47).contains(&k) => (vec![3], None),
            _ => (vec![], None),
        }
    }

    fn fixture() -> (Vec<Game>, Vec<BoxLine>) {
        let pairs = [[(1, 2), (3, 4)], [(1, 3), (2, 4)], [(1, 4), (2, 3)]];
        let mut games = Vec::new();
        let mut lines = Vec::new();
        let mut count = [0usize; 5];
        let mut id = 0i64;
        for round in 0..ROUNDS {
            for &(a, b) in &pairs[round % 3] {
                let (home, away) = if round % 2 == 0 { (a, b) } else { (b, a) };
                id += 1;
                let mut scores = [0i32; 2];
                for (side, team) in [home, away].into_iter().enumerate() {
                    let k = count[team as usize];
                    count[team as usize] += 1;
                    let (sitting, star_minutes) = roster_rule(team, k);
                    let mut minutes: Vec<i32> = SLOTS.iter().map(|s| s[0]).collect();
                    if let Some(m) = star_minutes {
                        minutes[0] = m;
                    }
                    for &s in &sitting {
                        minutes[s] = 0;
                    }
                    for (slot, profile) in SLOTS.iter().enumerate() {
                        if sitting.contains(&slot) {
                            continue;
                        }
                        let seconds = minutes[slot] * 60;
                        // Fewer minutes, the same rates: the counting statistics scale.
                        let profile: Vec<i32> = profile
                            .iter()
                            .map(|v| {
                                (f64::from(*v) * f64::from(minutes[slot]) / f64::from(profile[0]))
                                    .round() as i32
                            })
                            .collect();
                        scores[side] += profile[1];
                        lines.push(BoxLine {
                            game_id: id,
                            player_id: PlayerId::new(team * 100 + slot as i32 + 1),
                            team_id: TeamId::new(team),
                            seconds: Some(seconds),
                            pts: Some(profile[1]),
                            reb: Some(profile[2] + profile[3]),
                            oreb: Some(profile[2]),
                            dreb: Some(profile[3]),
                            ast: Some(profile[4]),
                            stl: Some(profile[5]),
                            blk: Some(profile[6]),
                            tov: Some(profile[7]),
                            pf: Some(profile[8]),
                            fgm: Some(profile[9]),
                            fga: Some(profile[10]),
                            fg3m: Some(profile[11]),
                            ftm: Some(profile[12]),
                            fta: Some(profile[13]),
                        });
                    }
                }
                games.push(game(id, id, home, away, scores[0], scores[1]));
            }
        }
        (games, lines)
    }

    fn returns() -> SeasonReturns {
        let (games, lines) = fixture();
        let table = bpm::table(S, &games, &lines, &HashMap::new());
        assert!(table.bpm_available, "{:?}", table.note);
        season_returns(S, &games, &lines, &table)
    }

    #[test]
    fn brackets_and_lengths_follow_their_edges() {
        assert_eq!(Bracket::of(-0.1), Bracket::BelowZero);
        assert_eq!(Bracket::of(0.0), Bracket::ZeroToTwo);
        assert_eq!(Bracket::of(1.99), Bracket::ZeroToTwo);
        assert_eq!(Bracket::of(2.0), Bracket::TwoToFive);
        assert_eq!(Bracket::of(5.0), Bracket::FiveAndUp);
        assert_eq!(Length::of(5), Length::FiveToNine);
        assert_eq!(Length::of(9), Length::FiveToNine);
        assert_eq!(Length::of(10), Length::TenToNineteen);
        assert_eq!(Length::of(20), Length::TwentyToThirtyNine);
        assert_eq!(Length::of(40), Length::FortyAndUp);
        assert_eq!(parameters().brackets.len(), 4);
        assert_eq!(
            serde_json::to_value(Bracket::FiveAndUp).unwrap(),
            "BPM 5 and up"
        );
    }

    #[test]
    fn a_run_with_a_return_is_an_event_and_the_other_gaps_are_not() {
        let r = returns();
        assert_eq!(r.team_games, 4 * ROUNDS as u32);
        assert_eq!(r.player_teams, 32);
        let who: Vec<(i32, u32)> = r
            .events
            .iter()
            .map(|e| (e.player_id.as_i32(), e.missed))
            .collect();
        // The star of team 1 (12 games) and the fourth man of team 4 (7 games). The
        // three-game gap, the run to the end of the season and the run too early for a
        // window before it are not events.
        assert_eq!(who, vec![(101, 12), (404, 7), (305, 5), (202, 6)]);
        assert!(r.note.is_none());
        // The fifth man of team 3 sat again seven games after his return: his early and
        // late windows show it, and that second run is no event of its own.
        let again = &r.events[2];
        assert_eq!(again.early.played, 7);
        assert_eq!(again.late.unwrap().played, 5);
        let star = &r.events[0];
        assert_eq!(star.length, Length::TenToNineteen);
        assert_eq!(star.team_id, TeamId::new(1));
        assert_eq!(r.events[1].length, Length::FiveToNine);
        // Fourteen team games remained after the late return: ten early, four late.
        let late_return = &r.events[3];
        assert_eq!(late_return.early.team_games, 10);
        assert_eq!(late_return.late.unwrap().team_games, 4);
    }

    #[test]
    fn the_windows_read_the_minutes_and_the_worth_around_the_run() {
        let r = returns();
        let star = &r.events[0];
        assert_eq!(star.pre.team_games, 20);
        assert_eq!(star.pre.played, 20);
        assert!(
            (star.pre.minutes_per_game - 36.0).abs() < 0.5,
            "{}",
            star.pre.minutes_per_game
        );
        assert_eq!(star.early.team_games, 10);
        assert_eq!(star.early.played, 10);
        assert!(
            (star.early.minutes_per_game - 24.0).abs() < 0.5,
            "{}",
            star.early.minutes_per_game
        );
        let late = star.late.expect("twenty games remained");
        assert_eq!(late.team_games, 10);
        assert!((late.minutes_per_game - 36.0).abs() < 0.5);
        // Fewer minutes at the same rates: a smaller share and a smaller worth per game.
        assert!(star.early.share.unwrap() < star.pre.share.unwrap());
        assert!(star.early.worth_elo.unwrap() < star.pre.worth_elo.unwrap());
        assert!(
            star.drop_early().unwrap() < -20.0,
            "{:?}",
            star.drop_early()
        );
        // Back to his minutes: back to his worth.
        assert!(
            star.drop_late().unwrap().abs() < 5.0,
            "{:?}",
            star.drop_late()
        );
        assert_eq!(star.bracket, Bracket::of(star.pre.bpm.unwrap()));
        assert!(
            star.pre.bpm.unwrap() > 2.0,
            "a 26-point scorer rates well: {}",
            star.pre.bpm.unwrap()
        );
        // The dates are the team's games, not the league's.
        assert!(star.first_missed < star.returned);
    }

    #[test]
    fn the_controls_are_the_clean_stretches_away_from_the_runs() {
        let r = returns();
        let of = |id: i32| {
            r.controls
                .iter()
                .filter(|c| c.player_id == PlayerId::new(id))
                .collect::<Vec<_>>()
        };
        // A healthy regular has a stretch at every tenth game: 0, 10, ..., 60.
        assert_eq!(of(201).len(), 7);
        // The star of team 1: his windows block 5..57, so only the stretch at 60 remains.
        assert_eq!(of(101).len(), 1);
        // The fourth man of team 4: his windows block 20..67; no forty games lie clear.
        assert!(of(404).is_empty());
        // Out from game 70 to the end, a run without a return still blocks 50..100.
        assert_eq!(of(301).len(), 2);
        // The second man of team 2: his late run blocks 60..100; his three-game gap at
        // 30..33 is no run, so the stretch at 20 stands with seventeen games played.
        let second = of(202);
        assert_eq!(second.len(), 3);
        assert!(second.iter().any(|c| c.pre.played == 17));
        // Full windows, a regular player, the bracket of his first twenty games.
        for c in &r.controls {
            assert_eq!((c.early.team_games, c.late.team_games), (10, 10));
            assert!(c.pre.played >= MIN_PRE_PLAYED);
            assert_eq!(c.bracket, Bracket::of(c.pre.bpm.unwrap()));
        }
        // A team-mate's absence raises the share of the players left, so a stretch that
        // spans one shows a drop or a rise; the stretch 40..80 of team 2's star spans
        // none, and his steady profile shows none.
        let steady = of(201)[4];
        assert!(
            steady.drop_early().unwrap().abs() < 1.0,
            "{:?}",
            steady.drop_early()
        );
        assert!(steady.drop_late().unwrap().abs() < 1.0);
        let spanning = of(201)[0];
        assert!(
            spanning.drop_late().unwrap().abs() > 1.0,
            "{:?}",
            spanning.drop_late()
        );
        assert!(r.controls.iter().any(|c| c.bracket == Bracket::FiveAndUp));
        assert!(r.controls.iter().any(|c| c.bracket == Bracket::BelowZero));
    }

    #[test]
    fn the_cells_hold_the_total_and_the_groups_with_events() {
        let r = returns();
        let cells = cells(&r.events, &r.controls);
        let total = &cells[0];
        assert_eq!((total.bracket, total.length, total.events), (None, None, 4));
        assert_eq!(total.late_events, 4);
        // The samples of the brackets the three events fall in.
        let brackets: std::collections::BTreeSet<Bracket> =
            r.events.iter().map(|e| e.bracket).collect();
        let expected: u32 = r
            .controls
            .iter()
            .filter(|c| brackets.contains(&c.bracket))
            .count() as u32;
        assert!(expected > 0);
        assert_eq!(total.controls, expected);
        assert!(total.drop_early.unwrap() < 0.0);
        assert!(total.drop_early_se.is_some());
        // The controls' drop is small beside the events' (team-mates' absences move the
        // shares a little; a return moves the star's minutes a lot), and the effect's
        // error carries the controls' error too.
        assert!(total.control_drop_early.unwrap().abs() < 10.0);
        assert!(total.effect_early.unwrap() < 0.0);
        assert!(total.effect_early_se.unwrap() >= total.drop_early_se.unwrap());
        assert!(total.effect_late.is_some());
        // A cell whose bracket has fewer than two control samples has no effect.
        let orphan = ReturnEvent {
            bracket: Bracket::TwoToFive,
            ..r.events[0].clone()
        };
        let none: Vec<ControlSample> = r
            .controls
            .iter()
            .filter(|c| c.bracket != Bracket::TwoToFive)
            .cloned()
            .collect();
        let without = cell(None, None, &[&orphan], &bracket_controls(&none));
        assert!(without.effect_early.is_none());
        assert!(without.control_drop_early.is_none());
        assert_eq!(without.controls, 0);
        let mut one = none.clone();
        one.push(ControlSample {
            bracket: Bracket::TwoToFive,
            ..r.controls[0].clone()
        });
        let single = cell(None, None, &[&orphan], &bracket_controls(&one));
        assert_eq!(single.controls, 1);
        assert!(single.effect_early.is_none());
        assert!(single.effect_late.is_none());
        let by_length: Vec<&Cell> = cells
            .iter()
            .filter(|c| c.bracket.is_none() && c.length.is_some())
            .collect();
        assert_eq!(by_length.len(), 2);
        let ten = by_length
            .iter()
            .find(|c| c.length == Some(Length::TenToNineteen))
            .unwrap();
        assert_eq!(ten.events, 1);
        assert!(
            ten.drop_early_se.is_none(),
            "one event has no standard error"
        );
        assert!((ten.early_availability.unwrap() - 1.0).abs() < 1e-9);
        // Every bracket-by-length cell has at least one event.
        assert!(cells
            .iter()
            .filter(|c| c.bracket.is_some() && c.length.is_some())
            .all(|c| c.events >= 1));
    }

    #[test]
    fn a_season_without_bpm_has_a_note_and_no_events() {
        let (games, mut lines) = fixture();
        for l in &mut lines {
            l.stl = None;
            l.blk = None;
        }
        let table = bpm::table(S, &games, &lines, &HashMap::new());
        assert!(!table.bpm_available);
        let r = season_returns(S, &games, &lines, &table);
        assert!(r.events.is_empty());
        assert!(r.note.is_some());
        let only_total = cells(&r.events, &r.controls);
        assert!(r.controls.is_empty());
        assert_eq!(only_total.len(), 1);
        assert_eq!(only_total[0].events, 0);
        assert!(only_total[0].drop_early.is_none());
    }

    #[test]
    fn the_mean_and_its_error_follow_the_sample() {
        assert_eq!(mean_se(&[]), (None, None));
        assert_eq!(mean_se(&[3.0]), (Some(3.0), None));
        let (m, se) = mean_se(&[1.0, 3.0]);
        assert!((m.unwrap() - 2.0).abs() < 1e-12);
        assert!((se.unwrap() - 1.0).abs() < 1e-12);
        assert!(mean(std::iter::empty()).is_none());
    }
}
