//! Reconstruct historical playoff series, results and the 16-team bracket from game records.

use std::collections::HashMap;

use chrono::NaiveDate;

use crate::domain::models::{Conference, Game, GameType, PlayoffResult, Round, Team};
use crate::domain::reference;

use super::bracket::{Bracket, BracketTeam, PlayoffFormat, SeriesRecord, SeriesState, Side, SLOT_SEEDS};
use super::seeding;
use super::standings::{GameResult, StandingsTable};

/// One historical series, assembled from its games.
#[derive(Debug, Clone)]
pub struct Series {
    pub team_a: i32,
    pub team_b: i32,
    pub state: SeriesState,
    pub first_date: NaiveDate,
    pub last_date: NaiveDate,
    pub game1_host: i32,
    pub neutral: bool,
    /// Round counted from the start of the postseason (1 = first series a team plays).
    pub round_from_start: u8,
    /// Depth counted back from the Finals (0 = Finals); only set for complete postseasons.
    pub depth: Option<u8>,
}

impl Series {
    pub fn winner(&self) -> Option<i32> {
        self.state.winner().map(|s| self.team(s))
    }
    pub fn loser(&self) -> Option<i32> {
        self.state.winner().map(|s| self.team(s.other()))
    }
    pub fn team(&self, side: Side) -> i32 {
        match side {
            Side::A => self.team_a,
            Side::B => self.team_b,
        }
    }
    pub fn involves(&self, t: i32) -> bool {
        self.team_a == t || self.team_b == t
    }
    pub fn wins(&self) -> (u8, u8) {
        self.state.wins()
    }
    /// Round of this series: exact when the postseason is complete, otherwise counted from
    /// the front (which is exact for the 16-team era).
    pub fn round(&self) -> Round {
        match self.depth {
            Some(d) => Round::from_depth(d),
            None => Round::from_number(self.round_from_start.min(4)).unwrap_or(Round::Finals),
        }
    }
}

/// Group playoff games into series, in order of first game, and assign rounds/depths.
/// The season decides the series length of the first round (best-of-five before 2003).
pub fn series_from_games(season: i32, games: &[Game]) -> Vec<Series> {
    struct Raw {
        team_a: i32,
        team_b: i32,
        winners: Vec<Side>,
        first_date: NaiveDate,
        last_date: NaiveDate,
        game1_host: i32,
        neutral: bool,
    }
    let mut map: HashMap<(i32, i32), Raw> = HashMap::new();
    for g in games.iter().filter(|g| g.is_playoff()) {
        let key = (g.home_team_id.min(g.away_team_id), g.home_team_id.max(g.away_team_id));
        let raw = map.entry(key).or_insert_with(|| Raw {
            team_a: g.home_team_id,
            team_b: g.away_team_id,
            winners: Vec::new(),
            first_date: g.game_date,
            last_date: g.game_date,
            game1_host: g.home_team_id,
            neutral: g.neutral,
        });
        raw.last_date = raw.last_date.max(g.game_date);
        if let Some(home_won) = g.home_won() {
            raw.winners.push(if (g.home_team_id == raw.team_a) == home_won { Side::A } else { Side::B });
        }
    }
    let mut raws: Vec<Raw> = map.into_values().collect();
    raws.sort_by_key(|s| (s.first_date, s.team_a.min(s.team_b)));
    let format = PlayoffFormat::for_season(season);
    // rounds from the front: 1 + max(round of each participant's previous series)
    let mut last_round: HashMap<i32, u8> = HashMap::new();
    let mut series: Vec<Series> = raws
        .into_iter()
        .map(|raw| {
            let r = 1 + last_round.get(&raw.team_a).copied().unwrap_or(0).max(last_round.get(&raw.team_b).copied().unwrap_or(0));
            last_round.insert(raw.team_a, r);
            last_round.insert(raw.team_b, r);
            let games_in_series = if r == 1 { format.first_round_games } else { 7 };
            let state = raw.winners.iter().fold(SeriesState::best_of(games_in_series), |st, &w| st.record(w));
            Series {
                team_a: raw.team_a,
                team_b: raw.team_b,
                state,
                first_date: raw.first_date,
                last_date: raw.last_date,
                game1_host: raw.game1_host,
                neutral: raw.neutral,
                round_from_start: r,
                depth: None,
            }
        })
        .collect();
    // depths from the back when complete (single elimination: n_series == n_teams - 1, all decided)
    let n_teams = {
        let mut t: Vec<i32> = series.iter().flat_map(|s| [s.team_a, s.team_b]).collect();
        t.sort_unstable();
        t.dedup();
        t.len()
    };
    let complete = !series.is_empty() && series.len() + 1 == n_teams && series.iter().all(|s| s.winner().is_some());
    if complete {
        let mut next_depth: HashMap<i32, u8> = HashMap::new();
        for s in series.iter_mut().rev() {
            let w = s.winner().expect("complete");
            let d = next_depth.get(&w).map_or(0, |d| d + 1);
            s.depth = Some(d);
            next_depth.insert(s.team_a, d);
            next_depth.insert(s.team_b, d);
        }
    }
    series
}

pub fn postseason_complete(series: &[Series]) -> bool {
    !series.is_empty() && series.iter().all(|s| s.depth.is_some())
}

/// Result for every team that played a playoff game: the best stage it reached.
pub fn playoff_results(series: &[Series]) -> HashMap<i32, PlayoffResult> {
    let mut out: HashMap<i32, PlayoffResult> = HashMap::new();
    for s in series {
        let round = s.round();
        for t in [s.team_a, s.team_b] {
            let label = if s.winner() == Some(t) { round.won() } else { round.eliminated() };
            let e = out.entry(t).or_insert(label);
            *e = (*e).max(label);
        }
    }
    out
}

/// (champion, runner-up) of a complete postseason.
pub fn champion(series: &[Series]) -> Option<(i32, i32)> {
    let finals = series.iter().find(|s| s.depth == Some(0))?;
    Some((finals.winner()?, finals.loser()?))
}

/// Alignment map for a season: team id -> (conference, division).
pub fn alignment_map(season: i32, teams: &HashMap<i32, Team>, games: &[Game]) -> HashMap<i32, (Conference, String)> {
    let mut out = HashMap::new();
    for t in games.iter().flat_map(|g| [g.home_team_id, g.away_team_id]) {
        if out.contains_key(&t) {
            continue;
        }
        if let Some((c, d)) = teams.get(&t).and_then(|team| reference::alignment(season, &team.code)) {
            out.insert(t, (c.parse::<Conference>().expect("reference conference"), d.to_string()));
        }
    }
    out
}

/// Standings from the regular-season games in `games` (any game dated after `cutoff` is ignored).
pub fn standings(games: &[Game], alignment: &HashMap<i32, (Conference, String)>, cutoff: Option<NaiveDate>) -> StandingsTable {
    let mut table = StandingsTable::new(alignment);
    let idx = table.index();
    for g in games.iter().filter(|g| g.game_type == GameType::Regular && cutoff.is_none_or(|c| g.game_date <= c)) {
        if let Some(o) = g.outcome() {
            table.record(&idx, &GameResult { home: g.home_team_id, away: g.away_team_id, home_won: o.home_won(), margin: o.margin() });
        }
    }
    table
}

/// Elo of each team immediately before the playoffs (after its last non-playoff game).
pub fn pre_playoff_elo(games: &[Game]) -> HashMap<i32, f64> {
    games.iter().filter(|g| !g.is_playoff() && g.is_played()).fold(HashMap::new(), |mut m, g| {
        m.insert(g.home_team_id, g.home_elo_post);
        m.insert(g.away_team_id, g.away_elo_post);
        m
    })
}

#[derive(Debug, Clone)]
pub struct Reconstruction {
    pub bracket: Bracket,
    pub series: Vec<Series>,
    pub results: HashMap<i32, PlayoffResult>,
    pub complete: bool,
}

#[derive(Debug, Clone, thiserror::Error)]
pub enum ReconstructError {
    #[error("no conference alignment data for season {0} (supported from 2002-03 onward)")]
    NoAlignment(i32),
    #[error("season {season} has {found} first-round series in the data; a 16-team bracket needs 8")]
    IncompleteFirstRound { season: i32, found: usize },
    #[error("season {season}: expected 4 first-round series in the {conference}, found {found}")]
    ConferenceMismatch { season: i32, conference: Conference, found: usize },
    #[error("season {0}: a first-round series crosses conferences")]
    CrossConference(i32),
    #[error("unknown team id {0}")]
    UnknownTeam(i32),
}

/// Build the historically correct bracket for a season (16-team era, 2003+ alignment).
pub fn reconstruct(season: i32, games: &[Game], teams: &HashMap<i32, Team>) -> Result<Reconstruction, ReconstructError> {
    let alignment = alignment_map(season, teams, games);
    if alignment.is_empty() {
        return Err(ReconstructError::NoAlignment(season));
    }
    let series = series_from_games(season, games);
    let round1: Vec<&Series> = series.iter().filter(|s| s.round_from_start == 1).collect();
    if round1.len() != 8 {
        return Err(ReconstructError::IncompleteFirstRound { season, found: round1.len() });
    }
    let round2: Vec<&Series> = series.iter().filter(|s| s.round_from_start == 2).collect();
    let table = standings(games, &alignment, None);
    let elo = pre_playoff_elo(games);
    let results = playoff_results(&series);
    let complete = postseason_complete(&series);
    let mut notes = Vec::new();
    let mut slots: Vec<Option<BracketTeam>> = vec![None; 16];
    let mut team_slot: HashMap<i32, usize> = HashMap::new();

    for conf in [Conference::East, Conference::West] {
        let order = table.conference_order(conf, &HashMap::new());
        let seeds = seeding::seed_map(&seeding::seed_order(season, &order));
        let rank = |t: i32| seeds.get(&t).copied().unwrap_or(99);
        let conf_series: Vec<&Series> =
            round1.iter().copied().filter(|s| alignment.get(&s.team_a).is_some_and(|a| a.0 == conf)).collect();
        if conf_series.len() != 4 {
            return Err(ReconstructError::ConferenceMismatch { season, conference: conf, found: conf_series.len() });
        }
        if conf_series.iter().any(|s| alignment.get(&s.team_b).is_none_or(|a| a.0 != conf)) {
            return Err(ReconstructError::CrossConference(season));
        }
        let best_of = |s: &Series| rank(s.team_a).min(rank(s.team_b));
        // Series containing the top-ranked team
        let top_idx = (0..4).min_by_key(|&i| best_of(conf_series[i])).expect("four series");
        // Its partner (the 4-5 series): from round-2 data if available, else by rank
        let partner = conf_series[top_idx]
            .winner()
            .and_then(|w| round2.iter().find(|s| s.involves(w)))
            .and_then(|r2| {
                let other = if r2.team_a == conf_series[top_idx].team_a || r2.team_a == conf_series[top_idx].team_b {
                    r2.team_b
                } else {
                    r2.team_a
                };
                (0..4).find(|&i| i != top_idx && conf_series[i].involves(other))
            });
        let remaining: Vec<usize> = (0..4).filter(|&i| i != top_idx).collect();
        let partner = partner.unwrap_or_else(|| {
            let mut rem = remaining.clone();
            rem.sort_by_key(|&i| best_of(conf_series[i]));
            notes.push(format!("{conf}: bracket half assignment inferred from standings (no second-round data)"));
            rem[2]
        });
        let others: Vec<usize> = remaining.into_iter().filter(|&i| i != partner).collect();
        let (two_seven, three_six) =
            if best_of(conf_series[others[0]]) <= best_of(conf_series[others[1]]) { (others[0], others[1]) } else { (others[1], others[0]) };
        // positions: [1,8,4,5,2,7,3,6]
        for (si, pos) in [(top_idx, 0usize), (partner, 2), (two_seven, 4), (three_six, 6)] {
            let s = conf_series[si];
            let (hi, lo) = if rank(s.team_a) <= rank(s.team_b) { (s.team_a, s.team_b) } else { (s.team_b, s.team_a) };
            for (t, p) in [(hi, pos), (lo, pos + 1)] {
                let slot = conf.index() * 8 + p;
                let team = teams.get(&t).ok_or(ReconstructError::UnknownTeam(t))?;
                let (w, l) = table.rows.iter().find(|r| r.team_id == t).map_or((0, 0), |r| (r.wins, r.losses));
                let seed = SLOT_SEEDS[p];
                // Seeds 7 and 8 are decided by the play-in tournament, not by the standings.
                let play_in_seed = season >= 2020 && seed >= 7;
                if rank(t) != seed && !play_in_seed {
                    notes.push(format!("{}: bracket position implies seed {seed}; standings rule gives {}", team.code, rank(t)));
                }
                slots[slot] = Some(BracketTeam {
                    team_id: t,
                    code: team.code.clone(),
                    name: team.name.clone(),
                    franchise_key: team.franchise_key().to_string(),
                    conference: conf,
                    seed,
                    elo: elo.get(&t).copied().unwrap_or(1500.0),
                    wins: w,
                    losses: l,
                    win_pct: if w + l > 0 { f64::from(w) / f64::from(w + l) } else { 0.0 },
                    actual_result: results.get(&t).copied(),
                });
                team_slot.insert(t, slot);
            }
        }
    }
    let bracket_teams: Vec<BracketTeam> = slots.into_iter().map(|s| s.expect("all 16 slots filled")).collect();
    let actual_series = series
        .iter()
        .filter_map(|s| {
            let (&a, &b) = (team_slot.get(&s.team_a)?, team_slot.get(&s.team_b)?);
            let round = Bracket::meeting_round(a, b)?;
            if round != s.round() {
                notes.push(format!("series {}-{} played as {:?} but bracket implies {round:?}", s.team_a, s.team_b, s.round()));
            }
            let (wins_a, wins_b) = s.wins();
            Some(SeriesRecord {
                round,
                slot_a: a,
                slot_b: b,
                winner_slot: s.winner().and_then(|w| team_slot.get(&w).copied()),
                wins_a,
                wins_b,
                game1_host_slot: team_slot.get(&s.game1_host).copied().unwrap_or(a),
            })
        })
        .collect();
    let mut format = PlayoffFormat::for_season(season);
    if series.iter().all(|s| s.neutral) {
        format.neutral_site = true;
    }
    Ok(Reconstruction { bracket: Bracket { season, teams: bracket_teams, format, actual_series, notes }, series, results, complete })
}
