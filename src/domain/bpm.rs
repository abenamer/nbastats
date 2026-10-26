//! Box-score worth: Box Plus/Minus 2.0, Value Over Replacement Player and the Player
//! Efficiency Rating from a season's stored box scores, and the conversion of VORP to the
//! Elo scale of the ratings.
//!
//! The method is Daniel Myers's published BPM 2.0 (season level): per-100-possession
//! statistics weighted by coefficients that vary with the player's estimated position and
//! offensive role, plus a team adjustment that makes a team's minutes-weighted sum equal
//! its adjusted efficiency. The coefficients and the regressions for position and role
//! are the published ones; see `docs/concepts/box-score-worth.md` for what the application
//! had to choose itself (the shooting baseline, the lead estimate, the schedule
//! adjustment). PER is John Hollinger's formula with the league scaled to 15.
//!
//! Everything here is pure: the input is the season's games, its box-score lines and the
//! listed positions, the output a table.

use std::collections::{BTreeMap, HashMap};

use serde::{Deserialize, Serialize};

use super::ids::{PlayerId, SeasonYear, TeamId};
use super::models::{Game, GameType};
use super::worth::ELO_PER_POINT;

/// Replacement level in points per 100 possessions, the VORP baseline.
pub const REPLACEMENT_LEVEL: f64 = -2.0;
/// Wins over replacement per point of VORP.
pub const WINS_PER_VORP: f64 = 2.7;
/// Points per 100 possessions a team plays worse per point of lead (Engelmann); half of
/// the effect belongs to the team.
pub const LEAD_EFFECT: f64 = 0.35;
/// The listed position and the role prior each weigh this many minutes against the
/// season's regression estimate.
pub const SMOOTHING_MINUTES: f64 = 50.0;
/// The offensive role the smoothing minutes assume.
pub const ROLE_PRIOR: f64 = 4.0;
/// Threshold shooting: points per true shot attempt below the team's figure.
pub const THRESHOLD_BELOW_TEAM: f64 = 0.33;
/// Free throws per true shot attempt.
pub const FTA_WEIGHT: f64 = 0.44;
/// The age the turnover imputation assumes, since the store has no birth dates.
pub const IMPUTED_AGE: f64 = 27.0;
/// Turnovers per true shot attempt assumed for a team's possessions when the box score
/// has no turnovers at all.
pub const TOV_PER_TSA_FALLBACK: f64 = 0.17;
/// Offensive rebound share assumed for a team's possessions when the box score does not
/// split rebounds.
pub const ORB_SHARE_FALLBACK: f64 = 0.3;
/// The season model version of this table.
pub const BOX_MODEL_VERSION: &str = "bpm-2.0-season";

const SOS_ITERATIONS: usize = 30;

/// One player's full line in one game.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BoxLine {
    pub game_id: i64,
    pub player_id: PlayerId,
    pub team_id: TeamId,
    pub seconds: Option<i32>,
    pub pts: Option<i32>,
    pub reb: Option<i32>,
    pub oreb: Option<i32>,
    pub dreb: Option<i32>,
    pub ast: Option<i32>,
    pub stl: Option<i32>,
    pub blk: Option<i32>,
    pub tov: Option<i32>,
    pub pf: Option<i32>,
    pub fgm: Option<i32>,
    pub fga: Option<i32>,
    pub fg3m: Option<i32>,
    pub ftm: Option<i32>,
    pub fta: Option<i32>,
}

/// Which statistics the season's box scores carry; BPM needs them all, PER degrades.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Coverage {
    pub minutes: bool,
    pub rebound_split: bool,
    pub steals: bool,
    pub blocks: bool,
    pub turnovers: bool,
}

impl Coverage {
    /// BPM 2.0 needs minutes, the rebound split, steals and blocks; turnovers are imputed
    /// when missing, as the published method does for 1974-77.
    pub fn bpm(self) -> bool {
        self.minutes && self.rebound_split && self.steals && self.blocks
    }
    /// PER needs minutes; a missing statistic enters as zero for everyone.
    pub fn per(self) -> bool {
        self.minutes
    }
    pub fn per_partial(self) -> bool {
        !(self.rebound_split && self.steals && self.blocks && self.turnovers)
    }
}

/// A team's season in the table.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TeamBox {
    pub team_id: TeamId,
    /// Regular-season games with box scores for both teams.
    pub games: u32,
    pub possessions: f64,
    /// Possessions per 48 minutes.
    pub pace: f64,
    /// Margin per 100 possessions, as played.
    pub net_rating: f64,
    /// The same adjusted for the schedule.
    pub rating: f64,
    pub offensive_rating: f64,
    pub defensive_rating: f64,
    /// Half the mean margin: the lead the team played with, on average.
    pub average_lead: f64,
    /// The rating plus the lead effect: what the players' BPM sum to.
    pub adjusted_rating: f64,
    /// The constant added to every player's raw BPM.
    pub team_adjustment: f64,
    pub offensive_adjustment: f64,
    /// Points per true shot attempt.
    pub pts_per_tsa: f64,
}

/// One player's season with one team.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlayerBox {
    pub player_id: PlayerId,
    pub team_id: TeamId,
    pub games: u32,
    pub minutes: f64,
    /// The share of the team's minutes he was on the floor (0 to 1).
    pub minutes_share: f64,
    /// 1 (point guard) to 5 (center), estimated.
    pub position: f64,
    /// 1 (creator) to 5 (receiver), estimated.
    pub offensive_role: f64,
    pub pts_per_100: f64,
    pub raw_bpm: f64,
    pub bpm: f64,
    pub obpm: f64,
    pub dbpm: f64,
    pub vorp: f64,
    /// VORP in wins over replacement.
    pub wins: f64,
    pub per: Option<f64>,
    /// Points per game the team gains over a replacement player in the games he plays:
    /// BPM over replacement, by his share of the floor in those games and the team's pace.
    /// Comparable to the player worth, which is per game of absence; not diluted by games
    /// missed, so the roster's worths do not sum to the team (the VORPs do).
    pub worth_points: f64,
    /// The same in Elo.
    pub worth_elo: f64,
    pub turnovers_imputed: bool,
}

/// League figures of the season, over the covered games.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct LeagueBox {
    pub pace: f64,
    pub pts_per_100: f64,
    pub pts_per_tsa: f64,
}

/// The box-score worth of a season.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BoxTable {
    pub season: SeasonYear,
    pub model: &'static str,
    /// Regular-season games with lines for both teams.
    pub games: u32,
    /// Played regular-season games without lines for both teams.
    pub games_without_lines: u32,
    /// Teams whose lines carry no minutes while the league's do: they have no players here.
    pub teams_without_minutes: u32,
    pub coverage: Coverage,
    pub bpm_available: bool,
    pub per_available: bool,
    pub per_partial: bool,
    pub league: LeagueBox,
    pub teams: Vec<TeamBox>,
    /// Sorted by VORP, highest first (by PER when BPM is unavailable).
    pub players: Vec<PlayerBox>,
    pub note: Option<String>,
}

/// The position value of a listed position: 1 for a point guard to 5 for a center;
/// `None` for an unknown label.
pub fn listed_position(label: &str) -> Option<f64> {
    let s = label.trim().to_ascii_uppercase();
    if s.is_empty() {
        return None;
    }
    let parts: Vec<f64> = s
        .split(['-', '/'])
        .filter_map(|p| match p.trim() {
            "PG" => Some(1.0),
            "SG" => Some(2.0),
            "G" => Some(1.5),
            "SF" => Some(3.0),
            "PF" => Some(4.0),
            "F" => Some(3.5),
            "C" => Some(5.0),
            _ => None,
        })
        .collect();
    if parts.is_empty() {
        None
    } else {
        Some(parts.iter().sum::<f64>() / parts.len() as f64)
    }
}

/// Counting totals of a set of lines, with how many lines carried each optional statistic.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct Totals {
    lines: u32,
    seconds: f64,
    pts: f64,
    reb: f64,
    oreb: f64,
    dreb: f64,
    ast: f64,
    stl: f64,
    blk: f64,
    tov: f64,
    pf: f64,
    fgm: f64,
    fga: f64,
    fg3m: f64,
    ftm: f64,
    fta: f64,
    with_seconds: u32,
    with_split: u32,
    with_stl: u32,
    with_blk: u32,
    with_tov: u32,
}

impl Totals {
    fn add(&mut self, l: &BoxLine) {
        let f = |v: Option<i32>| f64::from(v.unwrap_or(0));
        self.lines += 1;
        self.seconds += f(l.seconds);
        self.pts += f(l.pts);
        self.reb += f(l.reb.or_else(|| match (l.oreb, l.dreb) {
            (Some(o), Some(d)) => Some(o + d),
            _ => None,
        }));
        self.oreb += f(l.oreb);
        self.dreb += f(l.dreb);
        self.ast += f(l.ast);
        self.stl += f(l.stl);
        self.blk += f(l.blk);
        self.tov += f(l.tov);
        self.pf += f(l.pf);
        self.fgm += f(l.fgm);
        self.fga += f(l.fga);
        self.fg3m += f(l.fg3m);
        self.ftm += f(l.ftm);
        self.fta += f(l.fta);
        self.with_seconds += u32::from(l.seconds.is_some());
        self.with_split += u32::from(l.oreb.is_some() && l.dreb.is_some());
        self.with_stl += u32::from(l.stl.is_some());
        self.with_blk += u32::from(l.blk.is_some());
        self.with_tov += u32::from(l.tov.is_some());
    }
    fn tsa(&self) -> f64 {
        self.fga + FTA_WEIGHT * self.fta
    }
}

/// A team's half of a game's possession estimate (Basketball-Reference's formula).
fn own_possessions(t: &Totals, opp: &Totals, cov: Coverage) -> f64 {
    let orb_share = if cov.rebound_split && t.oreb + opp.dreb > 0.0 {
        t.oreb / (t.oreb + opp.dreb)
    } else {
        ORB_SHARE_FALLBACK
    };
    let tov = if cov.turnovers {
        t.tov
    } else {
        TOV_PER_TSA_FALLBACK * t.tsa()
    };
    t.fga + 0.4 * t.fta - 1.07 * orb_share * (t.fga - t.fgm) + tov
}

/// Possessions of a game, the same for both teams.
fn possessions(team: &Totals, opp: &Totals, cov: Coverage) -> f64 {
    0.5 * (own_possessions(team, opp, cov) + own_possessions(opp, team, cov))
}

#[derive(Debug, Clone, Copy)]
struct TeamGame {
    possessions: f64,
    margin: f64,
    pts: f64,
    allowed: f64,
    opponent: TeamId,
}

#[derive(Debug, Default)]
struct TeamSeason {
    totals: Totals,
    games: Vec<TeamGame>,
}

impl Totals {
    fn add_totals(&mut self, o: &Totals) {
        self.lines += o.lines;
        self.seconds += o.seconds;
        self.pts += o.pts;
        self.reb += o.reb;
        self.oreb += o.oreb;
        self.dreb += o.dreb;
        self.ast += o.ast;
        self.stl += o.stl;
        self.blk += o.blk;
        self.tov += o.tov;
        self.pf += o.pf;
        self.fgm += o.fgm;
        self.fga += o.fga;
        self.fg3m += o.fg3m;
        self.ftm += o.ftm;
        self.fta += o.fta;
        self.with_seconds += o.with_seconds;
        self.with_split += o.with_split;
        self.with_stl += o.with_stl;
        self.with_blk += o.with_blk;
        self.with_tov += o.with_tov;
    }
}

impl TeamSeason {
    fn possessions(&self) -> f64 {
        self.games.iter().map(|g| g.possessions).sum()
    }
    fn count(&self) -> u32 {
        self.games.len() as u32
    }
}

/// Position and role regressions: coefficients on shares of team statistics.
const POS_INTERCEPT: f64 = 2.130;
const POS_TRB: f64 = 8.668;
const POS_STL: f64 = -2.486;
const POS_PF: f64 = 0.992;
const POS_AST: f64 = -3.536;
const POS_BLK: f64 = 1.667;
const POS_OLD_INTERCEPT: f64 = 1.590;
const POS_OLD_TRB: f64 = 10.892;
const POS_OLD_PF: f64 = 1.468;
const POS_OLD_AST: f64 = -4.400;
const ROLE_INTERCEPT: f64 = 6.00;
const ROLE_AST: f64 = -6.642;
const ROLE_THRESHOLD: f64 = -8.544;

/// A coefficient that varies linearly from position (or role) 1 to 5.
#[derive(Debug, Clone, Copy)]
struct Slope(f64, f64);

impl Slope {
    fn at(self, p: f64) -> f64 {
        self.0 + (p - 1.0) / 4.0 * (self.1 - self.0)
    }
}

/// The regression coefficients of BPM or OBPM.
#[derive(Debug, Clone, Copy)]
struct Coefficients {
    pts: f64,
    fg3m: f64,
    ast: Slope,
    tov: Slope,
    orb: Slope,
    drb: Slope,
    stl: Slope,
    blk: Slope,
    pf: f64,
    /// By offensive role.
    fga: Slope,
    fta: Slope,
    /// The constant at position 1; zero from position 3 up.
    position_constant: f64,
    /// The constant at role 1; its negative at role 5.
    role_constant: f64,
}

const BPM: Coefficients = Coefficients {
    pts: 0.860,
    fg3m: 0.389,
    ast: Slope(0.580, 1.034),
    tov: Slope(-0.964, -0.964),
    orb: Slope(0.613, 0.181),
    drb: Slope(0.116, 0.181),
    stl: Slope(1.369, 1.008),
    blk: Slope(1.327, 0.703),
    pf: -0.367,
    fga: Slope(-0.560, -0.780),
    fta: Slope(-0.246, -0.343),
    position_constant: -0.818,
    role_constant: -2.774,
};

const OBPM: Coefficients = Coefficients {
    pts: 0.605,
    fg3m: 0.477,
    ast: Slope(0.476, 0.476),
    tov: Slope(-0.579, -0.882),
    orb: Slope(0.606, 0.422),
    drb: Slope(-0.112, 0.103),
    stl: Slope(0.177, 0.294),
    blk: Slope(0.725, 0.097),
    pf: -0.439,
    fga: Slope(-0.330, -0.472),
    fta: Slope(-0.145, -0.208),
    position_constant: -1.698,
    role_constant: -0.860,
};

/// A player's statistics per 100 team possessions, points already adjusted for the team's
/// shooting context.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Per100 {
    pub adj_pts: f64,
    pub fg3m: f64,
    pub ast: f64,
    pub tov: f64,
    pub orb: f64,
    pub drb: f64,
    pub stl: f64,
    pub blk: f64,
    pub pf: f64,
    pub fga: f64,
    pub fta: f64,
}

fn raw_with(c: &Coefficients, s: &Per100, position: f64, role: f64) -> f64 {
    let position_constant = if position < 3.0 {
        (3.0 - position) * (c.position_constant / 2.0)
    } else {
        0.0
    };
    let role_constant = (3.0 - role) * (c.role_constant / 2.0);
    c.pts * s.adj_pts
        + c.fg3m * s.fg3m
        + c.ast.at(position) * s.ast
        + c.tov.at(position) * s.tov
        + c.orb.at(position) * s.orb
        + c.drb.at(position) * s.drb
        + c.stl.at(position) * s.stl
        + c.blk.at(position) * s.blk
        + c.pf * s.pf
        + c.fga.at(role) * s.fga
        + c.fta.at(role) * s.fta
        + position_constant
        + role_constant
}

/// Raw BPM before the team adjustment, from per-100 statistics and the estimates.
pub fn raw_bpm(s: &Per100, position: f64, role: f64) -> f64 {
    raw_with(&BPM, s, position, role)
}

/// Raw offensive BPM before the team adjustment.
pub fn raw_obpm(s: &Per100, position: f64, role: f64) -> f64 {
    raw_with(&OBPM, s, position, role)
}

/// Turnovers per 100 possessions imputed from the other statistics (the published
/// 1974-77 estimator), at the assumed age.
pub fn imputed_turnovers(
    season: SeasonYear,
    pts: f64,
    trb: f64,
    fga: f64,
    fta: f64,
    ast: f64,
    pf: f64,
) -> f64 {
    let year = f64::from(season.end_year() % 100);
    (4.1119 - 0.0191 * IMPUTED_AGE - 0.0519 * year - 0.0494 * pts
        + 0.0442 * trb
        + 0.0957 * fga
        + 0.2056 * fta
        + 0.2216 * ast
        + 0.1972 * pf)
        .max(0.0)
}

/// VORP: BPM over replacement level, by the share of minutes and of the schedule.
pub fn vorp(bpm: f64, minutes_share: f64, team_games: u32) -> f64 {
    (bpm - REPLACEMENT_LEVEL) * minutes_share * f64::from(team_games) / 82.0
}

/// Shift a team's estimates by one constant so their minutes-weighted mean is 3.0 after
/// clamping to 1..5 (the published recursive step, solved by bisection).
pub fn centre_on_three(values: &mut [f64], weights: &[f64]) {
    let total: f64 = weights.iter().sum();
    if total <= 0.0 || values.is_empty() {
        return;
    }
    let mean = |c: f64| {
        values
            .iter()
            .zip(weights)
            .map(|(v, w)| (v + c).clamp(1.0, 5.0) * w)
            .sum::<f64>()
            / total
    };
    let (mut lo, mut hi) = (-8.0, 8.0);
    for _ in 0..64 {
        let mid = 0.5 * (lo + hi);
        if mean(mid) < 3.0 {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    let c = 0.5 * (lo + hi);
    for v in values.iter_mut() {
        *v = (*v + c).clamp(1.0, 5.0);
    }
}

/// Hollinger's PER inputs at the league level.
#[derive(Debug, Clone, Copy)]
struct PerLeague {
    factor: f64,
    vop: f64,
    drb_pct: f64,
    ft_per_pf: f64,
    fta_per_pf: f64,
    pace: f64,
}

impl PerLeague {
    fn new(lg: &Totals, pace: f64) -> Option<Self> {
        if lg.fgm <= 0.0 || lg.ftm <= 0.0 || lg.pf <= 0.0 || lg.reb <= 0.0 {
            return None;
        }
        let denom = lg.fga - lg.oreb + lg.tov + FTA_WEIGHT * lg.fta;
        if denom <= 0.0 {
            return None;
        }
        Some(Self {
            factor: 2.0 / 3.0 - (0.5 * (lg.ast / lg.fgm)) / (2.0 * (lg.fgm / lg.ftm)),
            vop: lg.pts / denom,
            drb_pct: (lg.reb - lg.oreb) / lg.reb,
            ft_per_pf: lg.ftm / lg.pf,
            fta_per_pf: lg.fta / lg.pf,
            pace,
        })
    }

    /// Unadjusted PER of a line against its team's assist ratio.
    fn uper(&self, p: &Totals, team: &Totals) -> f64 {
        let mp = p.seconds / 60.0;
        if mp <= 0.0 {
            return 0.0;
        }
        let ast_ratio = if team.fgm > 0.0 {
            team.ast / team.fgm
        } else {
            0.0
        };
        let v = self.vop;
        let d = self.drb_pct;
        (p.fg3m
            + (2.0 / 3.0) * p.ast
            + (2.0 - self.factor * ast_ratio) * p.fgm
            + p.ftm * 0.5 * (1.0 + (1.0 - ast_ratio) + (2.0 / 3.0) * ast_ratio)
            - v * p.tov
            - v * d * (p.fga - p.fgm)
            - v * FTA_WEIGHT * (FTA_WEIGHT + 0.56 * d) * (p.fta - p.ftm)
            + v * (1.0 - d) * (p.reb - p.oreb)
            + v * d * p.oreb
            + v * p.stl
            + v * d * p.blk
            - p.pf * (self.ft_per_pf - FTA_WEIGHT * self.fta_per_pf * v))
            / mp
    }
}

/// The box-score worth table of a season.
///
/// `listed` gives the listed position value (see [`listed_position`]) of the players that
/// have one; the estimate of the others leans on 3.0.
pub fn table(
    season: SeasonYear,
    games: &[Game],
    lines: &[BoxLine],
    listed: &HashMap<PlayerId, f64>,
) -> BoxTable {
    let empty = |note: &str, coverage: Coverage, without: u32| BoxTable {
        season,
        model: BOX_MODEL_VERSION,
        games: 0,
        games_without_lines: without,
        teams_without_minutes: 0,
        coverage,
        bpm_available: false,
        per_available: false,
        per_partial: false,
        league: LeagueBox::default(),
        teams: vec![],
        players: vec![],
        note: Some(note.to_string()),
    };
    let regular: BTreeMap<i64, &Game> = games
        .iter()
        .filter(|g| g.game_type == GameType::Regular && g.is_played())
        .map(|g| (g.id, g))
        .collect();
    // Team totals per game, from the lines of that game.
    let mut per_game: BTreeMap<i64, BTreeMap<TeamId, Totals>> = BTreeMap::new();
    for l in lines {
        if regular.contains_key(&l.game_id) {
            per_game
                .entry(l.game_id)
                .or_default()
                .entry(l.team_id)
                .or_default()
                .add(l);
        }
    }
    // A game counts when both teams have lines.
    let covered: BTreeMap<i64, &Game> = regular
        .iter()
        .filter(|(id, g)| {
            per_game
                .get(id)
                .is_some_and(|t| t.contains_key(&g.home_team_id) && t.contains_key(&g.away_team_id))
        })
        .map(|(id, g)| (*id, *g))
        .collect();
    let without = (regular.len() - covered.len()) as u32;
    if covered.is_empty() {
        return empty(
            "No regular-season game of this season has box scores for both teams.",
            Coverage::default(),
            without,
        );
    }
    let mut league = Totals::default();
    for l in lines.iter().filter(|l| covered.contains_key(&l.game_id)) {
        league.add(l);
    }
    let present = |n: u32| n * 2 > league.lines;
    let coverage = Coverage {
        minutes: present(league.with_seconds) && league.seconds > 0.0,
        rebound_split: present(league.with_split),
        steals: present(league.with_stl),
        blocks: present(league.with_blk),
        turnovers: present(league.with_tov),
    };
    if !coverage.minutes {
        return empty(
            "The box scores of this season carry no minutes, which every rate here needs.",
            coverage,
            without,
        );
    }

    // Team seasons: totals and the game list with possessions and margins.
    let mut teams: BTreeMap<TeamId, TeamSeason> = BTreeMap::new();
    for (id, g) in &covered {
        let totals = &per_game[id];
        let (home, away) = (&totals[&g.home_team_id], &totals[&g.away_team_id]);
        let poss = possessions(home, away, coverage).max(1.0);
        let (hs, as_) = (
            f64::from(g.home_score.unwrap_or(0)),
            f64::from(g.away_score.unwrap_or(0)),
        );
        for (team, own, pts, allowed, opp) in [
            (g.home_team_id, home, hs, as_, g.away_team_id),
            (g.away_team_id, away, as_, hs, g.home_team_id),
        ] {
            let season_entry = teams.entry(team).or_default();
            season_entry.games.push(TeamGame {
                possessions: poss,
                margin: pts - allowed,
                pts,
                allowed,
                opponent: opp,
            });
            season_entry.totals.add_totals(own);
        }
    }
    let league_poss: f64 = teams.values().map(TeamSeason::possessions).sum::<f64>() / 2.0;
    let league_team_minutes: f64 = league.seconds / 60.0 / 5.0;
    let league_pace = if league_team_minutes > 0.0 {
        league_poss * 2.0 / league_team_minutes * 48.0
    } else {
        0.0
    };
    let league_pts_per_100 = if league_poss > 0.0 {
        league.pts / (2.0 * league_poss) * 100.0
    } else {
        0.0
    };
    let league_pts_per_tsa = if league.tsa() > 0.0 {
        league.pts / league.tsa()
    } else {
        0.0
    };

    // Schedule-adjusted ratings: each team's offence against the defences it met, and
    // its defence against the offences, iterated to a fixed point.
    let ids: Vec<TeamId> = teams.keys().copied().collect();
    let index: HashMap<TeamId, usize> = ids.iter().enumerate().map(|(i, t)| (*t, i)).collect();
    let mut off = vec![0.0; ids.len()];
    let mut def = vec![0.0; ids.len()];
    for _ in 0..SOS_ITERATIONS {
        let mut next_off = vec![0.0; ids.len()];
        let mut next_def = vec![0.0; ids.len()];
        for (i, t) in ids.iter().enumerate() {
            let ts = &teams[t];
            let n = ts.games.len() as f64;
            for g in &ts.games {
                let o = index[&g.opponent];
                next_off[i] += (g.pts / g.possessions * 100.0 - league_pts_per_100 + def[o]) / n;
                next_def[i] +=
                    (league_pts_per_100 - g.allowed / g.possessions * 100.0 + off[o]) / n;
            }
        }
        // Keep the league centred: the adjustment is relative.
        let mo = next_off.iter().sum::<f64>() / ids.len() as f64;
        let md = next_def.iter().sum::<f64>() / ids.len() as f64;
        off = next_off.iter().map(|v| v - mo).collect();
        def = next_def.iter().map(|v| v - md).collect();
    }

    let mut team_rows: BTreeMap<TeamId, TeamBox> = BTreeMap::new();
    for (i, t) in ids.iter().enumerate() {
        let ts = &teams[t];
        let poss = ts.possessions();
        let n = ts.count();
        let margin: f64 = ts.games.iter().map(|g| g.margin).sum();
        let average_lead = margin / f64::from(n) / 2.0;
        let rating = off[i] + def[i];
        let team_minutes = ts.totals.seconds / 60.0 / 5.0;
        team_rows.insert(
            *t,
            TeamBox {
                team_id: *t,
                games: n,
                possessions: poss,
                pace: if team_minutes > 0.0 {
                    poss / team_minutes * 48.0
                } else {
                    0.0
                },
                net_rating: margin / poss * 100.0,
                rating,
                offensive_rating: off[i],
                defensive_rating: def[i],
                average_lead,
                adjusted_rating: rating + LEAD_EFFECT / 2.0 * average_lead,
                team_adjustment: 0.0,
                offensive_adjustment: 0.0,
                pts_per_tsa: if ts.totals.tsa() > 0.0 {
                    ts.totals.pts / ts.totals.tsa()
                } else {
                    league_pts_per_tsa
                },
            },
        );
    }

    // Player seasons with each team.
    let mut stints: BTreeMap<(TeamId, PlayerId), (Totals, u32)> = BTreeMap::new();
    for l in lines.iter().filter(|l| covered.contains_key(&l.game_id)) {
        let e = stints.entry((l.team_id, l.player_id)).or_default();
        e.0.add(l);
        e.1 += u32::from(l.seconds.unwrap_or(0) > 0);
    }
    let per_league = PerLeague::new(&league, league_pace);
    let per_available = per_league.is_some();
    let bpm_available = coverage.bpm();

    // Position and role estimates, team by team.
    struct Stint {
        player: PlayerId,
        team: TeamId,
        totals: Totals,
        games: u32,
        share: f64,
        position: f64,
        role: f64,
        per100: Per100,
        tov_imputed: bool,
    }
    let mut all: Vec<Stint> = Vec::new();
    let mut teams_without_minutes = 0u32;
    for t in &ids {
        let ts = &teams[t];
        let team_seconds = ts.totals.seconds;
        if team_seconds <= 0.0 {
            teams_without_minutes += 1;
            continue;
        }
        let team_poss = ts.possessions();
        let team_tsa = ts.totals.tsa();
        let team_ptsa = team_rows[t].pts_per_tsa;
        let team_threshold = THRESHOLD_BELOW_TEAM * team_tsa;
        let mut rows: Vec<Stint> = Vec::new();
        for ((team, player), (p, games)) in stints.range((*t, PlayerId::new(i32::MIN))..) {
            if team != t {
                break;
            }
            if p.seconds <= 0.0 {
                continue;
            }
            let share = p.seconds / (team_seconds / 5.0);
            let minutes = p.seconds / 60.0;
            // Shares of the team's statistics while he was on the floor.
            let pct = |own: f64, team: f64| {
                if team > 0.0 {
                    own / (team * share)
                } else {
                    0.0
                }
            };
            let pos_raw = if coverage.steals && coverage.blocks {
                POS_INTERCEPT
                    + POS_TRB * pct(p.reb, ts.totals.reb)
                    + POS_STL * pct(p.stl, ts.totals.stl)
                    + POS_PF * pct(p.pf, ts.totals.pf)
                    + POS_AST * pct(p.ast, ts.totals.ast)
                    + POS_BLK * pct(p.blk, ts.totals.blk)
            } else {
                POS_OLD_INTERCEPT
                    + POS_OLD_TRB * pct(p.reb, ts.totals.reb)
                    + POS_OLD_PF * pct(p.pf, ts.totals.pf)
                    + POS_OLD_AST * pct(p.ast, ts.totals.ast)
            };
            let listed_value = listed.get(player).copied().unwrap_or(3.0);
            let position = (pos_raw * minutes + listed_value * SMOOTHING_MINUTES)
                / (minutes + SMOOTHING_MINUTES);
            let threshold = p.pts - (team_ptsa - THRESHOLD_BELOW_TEAM) * p.tsa();
            let role_raw = ROLE_INTERCEPT
                + ROLE_AST * pct(p.ast, ts.totals.ast)
                + ROLE_THRESHOLD * pct(threshold, team_threshold);
            let role = (role_raw * minutes + ROLE_PRIOR * SMOOTHING_MINUTES)
                / (minutes + SMOOTHING_MINUTES);
            // Per 100 team possessions while on the floor.
            let on_floor = team_poss * share;
            let per100 = |v: f64| {
                if on_floor > 0.0 {
                    v / on_floor * 100.0
                } else {
                    0.0
                }
            };
            let (tov, tov_imputed) = if coverage.turnovers {
                (per100(p.tov), false)
            } else {
                (
                    imputed_turnovers(
                        season,
                        per100(p.pts),
                        per100(p.reb),
                        per100(p.fga),
                        per100(p.fta),
                        per100(p.ast),
                        per100(p.pf),
                    ),
                    true,
                )
            };
            rows.push(Stint {
                player: *player,
                team: *team,
                totals: *p,
                games: *games,
                share,
                position,
                role,
                per100: Per100 {
                    adj_pts: per100(p.pts) - per100(p.tsa()) * (team_ptsa - league_pts_per_tsa),
                    fg3m: per100(p.fg3m),
                    ast: per100(p.ast),
                    tov,
                    orb: per100(p.oreb),
                    drb: per100(p.dreb),
                    stl: per100(p.stl),
                    blk: per100(p.blk),
                    pf: per100(p.pf),
                    fga: per100(p.fga),
                    fta: per100(p.fta),
                },
                tov_imputed,
            });
        }
        let weights: Vec<f64> = rows.iter().map(|r| r.totals.seconds).collect();
        let mut positions: Vec<f64> = rows.iter().map(|r| r.position).collect();
        let mut roles: Vec<f64> = rows.iter().map(|r| r.role).collect();
        centre_on_three(&mut positions, &weights);
        centre_on_three(&mut roles, &weights);
        for ((r, p), role) in rows.iter_mut().zip(positions).zip(roles) {
            r.position = p;
            r.role = role;
        }
        all.extend(rows);
    }

    // Raw BPM, team adjustments, PER.
    let raw: Vec<(f64, f64)> = all
        .iter()
        .map(|r| {
            (
                raw_bpm(&r.per100, r.position, r.role),
                raw_obpm(&r.per100, r.position, r.role),
            )
        })
        .collect();
    for (t, row) in team_rows.iter_mut() {
        let (mut sum, mut osum) = (0.0, 0.0);
        for (r, (b, o)) in all.iter().zip(&raw) {
            if r.team == *t {
                sum += b * r.share;
                osum += o * r.share;
            }
        }
        // The shares sum to five (five players on the floor), so a constant added to every
        // player moves the weighted sum by five times itself.
        row.team_adjustment = (row.adjusted_rating - sum) / 5.0;
        row.offensive_adjustment =
            (row.offensive_rating + LEAD_EFFECT / 4.0 * row.average_lead - osum) / 5.0;
    }
    // PER: pace-adjusted, then scaled so the league's minutes-weighted mean is 15.
    let apers: Vec<f64> = all
        .iter()
        .map(|r| {
            per_league.map_or(0.0, |pl| {
                let team = &teams[&r.team];
                let team_pace = team_rows[&r.team].pace;
                let adj = if team_pace > 0.0 {
                    pl.pace / team_pace
                } else {
                    1.0
                };
                adj * pl.uper(&r.totals, &team.totals)
            })
        })
        .collect();
    let minutes_total: f64 = all.iter().map(|r| r.totals.seconds).sum();
    let league_aper = if minutes_total > 0.0 {
        all.iter()
            .zip(&apers)
            .map(|(r, a)| a * r.totals.seconds)
            .sum::<f64>()
            / minutes_total
    } else {
        0.0
    };

    let mut players: Vec<PlayerBox> = Vec::with_capacity(all.len());
    for ((r, (b, o)), aper) in all.iter().zip(&raw).zip(&apers) {
        let team = &team_rows[&r.team];
        let bpm = *b + team.team_adjustment;
        let obpm = *o + team.offensive_adjustment;
        let v = vorp(bpm, r.share, team.games);
        let pace_per_game = if team.games > 0 {
            team.possessions / f64::from(team.games)
        } else {
            0.0
        };
        // His share of the floor in the games he played, not over the whole season.
        let team_seconds_per_game = if team.games > 0 {
            teams[&r.team].totals.seconds / f64::from(team.games)
        } else {
            0.0
        };
        let share_when_playing = if r.games > 0 && team_seconds_per_game > 0.0 {
            r.totals.seconds / (f64::from(r.games) * team_seconds_per_game / 5.0)
        } else {
            0.0
        };
        let worth_points = (bpm - REPLACEMENT_LEVEL) * share_when_playing * pace_per_game / 100.0;
        let per = if per_available && league_aper > 0.0 {
            Some(aper * 15.0 / league_aper)
        } else {
            None
        };
        players.push(PlayerBox {
            player_id: r.player,
            team_id: r.team,
            games: r.games,
            minutes: r.totals.seconds / 60.0,
            minutes_share: r.share,
            position: r.position,
            offensive_role: r.role,
            pts_per_100: if team.possessions * r.share > 0.0 {
                r.totals.pts / (team.possessions * r.share) * 100.0
            } else {
                0.0
            },
            raw_bpm: if bpm_available { *b } else { 0.0 },
            bpm: if bpm_available { bpm } else { 0.0 },
            obpm: if bpm_available { obpm } else { 0.0 },
            dbpm: if bpm_available { bpm - obpm } else { 0.0 },
            vorp: if bpm_available { v } else { 0.0 },
            wins: if bpm_available {
                v * WINS_PER_VORP
            } else {
                0.0
            },
            per,
            worth_points: if bpm_available { worth_points } else { 0.0 },
            worth_elo: if bpm_available {
                worth_points * ELO_PER_POINT
            } else {
                0.0
            },
            turnovers_imputed: r.tov_imputed,
        });
    }
    if bpm_available {
        players.sort_by(|a, b| {
            b.vorp
                .total_cmp(&a.vorp)
                .then(a.player_id.cmp(&b.player_id))
        });
    } else {
        players.sort_by(|a, b| {
            b.per
                .unwrap_or(f64::MIN)
                .total_cmp(&a.per.unwrap_or(f64::MIN))
                .then(a.player_id.cmp(&b.player_id))
        });
    }
    let note = if !bpm_available {
        Some(
            "The box scores of this season lack the rebound split, steals or blocks, which BPM needs; only PER is shown, with the missing statistics as zero."
                .to_string(),
        )
    } else if !coverage.turnovers {
        Some("The box scores carry no turnovers: they are imputed from the other statistics, as the published method does for 1974-77.".to_string())
    } else {
        None
    };
    let note = match (note, teams_without_minutes) {
        (n, 0) => n,
        (None, k) => Some(format!(
            "{k} team(s) have lines without minutes and no players here."
        )),
        (Some(n), k) => Some(format!(
            "{n} {k} team(s) have lines without minutes and no players here."
        )),
    };
    BoxTable {
        season,
        model: BOX_MODEL_VERSION,
        games: covered.len() as u32,
        games_without_lines: without,
        teams_without_minutes,
        coverage,
        bpm_available,
        per_available,
        per_partial: per_available && coverage.per_partial(),
        league: LeagueBox {
            pace: league_pace,
            pts_per_100: league_pts_per_100,
            pts_per_tsa: league_pts_per_tsa,
        },
        teams: team_rows.into_values().collect(),
        players,
        note,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::models::{EloSource, ResultSource};
    use chrono::NaiveDate;

    fn game(id: i64, home: i32, away: i32, home_score: i32, away_score: i32) -> Game {
        Game {
            id,
            season: SeasonYear::of(2025),
            game_date: NaiveDate::from_ymd_opt(2024, 11, 1).unwrap() + chrono::Duration::days(id),
            seq: 0,
            home_team_id: TeamId::new(home),
            away_team_id: TeamId::new(away),
            neutral: false,
            game_type: GameType::Regular,
            home_score: Some(home_score),
            away_score: Some(away_score),
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

    /// A team's ten lines in a game: five starters at 36 minutes, five reserves at 12,
    /// with statistics that depend on the slot so the roster is not uniform. `edge`
    /// shifts the first starter's production.
    fn lines(game_id: i64, team: i32, edge: i32) -> Vec<BoxLine> {
        (0..10)
            .map(|slot| {
                let starter = slot < 5;
                let seconds = if starter { 36 * 60 } else { 12 * 60 };
                let k = if starter { 3 } else { 1 };
                let bonus = if slot == 0 { edge } else { 0 };
                BoxLine {
                    game_id,
                    player_id: PlayerId::new(team * 100 + slot),
                    team_id: TeamId::new(team),
                    seconds: Some(seconds),
                    pts: Some(4 * k + bonus),
                    reb: Some(k + (slot % 5)),
                    oreb: Some(if slot % 5 >= 3 { k } else { 0 }),
                    dreb: Some(k + (slot % 5) - if slot % 5 >= 3 { k } else { 0 }),
                    ast: Some(if slot % 5 == 0 { 3 * k } else { k / 3 }),
                    stl: Some(if slot % 5 <= 1 { 1 } else { 0 }),
                    blk: Some(if slot % 5 == 4 { k } else { 0 }),
                    tov: Some(1),
                    pf: Some(2),
                    fgm: Some(2 * k),
                    fga: Some(4 * k),
                    fg3m: Some(if slot % 5 == 1 { 1 } else { 0 }),
                    ftm: Some(bonus.max(0)),
                    fta: Some(bonus.max(0)),
                }
            })
            .collect()
    }

    /// Four teams, a double round robin, team 1 wins its games by eight and team 4 loses
    /// by eight; the others split.
    fn season() -> (Vec<Game>, Vec<BoxLine>) {
        let mut games = Vec::new();
        let mut all = Vec::new();
        let mut id = 0;
        for home in 1..=4 {
            for away in 1..=4 {
                if home == away {
                    continue;
                }
                let strength = |t: i32| match t {
                    1 => 4,
                    4 => -4,
                    _ => 0,
                };
                let margin = strength(home) - strength(away);
                games.push(game(
                    id,
                    home,
                    away,
                    100 + margin.max(0),
                    100 + (-margin).max(0),
                ));
                all.extend(lines(id, home, strength(home)));
                all.extend(lines(id, away, strength(away)));
                id += 1;
            }
        }
        (games, all)
    }

    #[test]
    fn the_published_example_reproduces_within_its_own_rounding() {
        // 2017 LeBron James, from the BPM 2.0 page: position 2.3, offensive role 1.0.
        let s = Per100 {
            adj_pts: 30.4,
            fg3m: 2.2,
            ast: 11.5,
            tov: 5.4,
            orb: 1.7,
            drb: 9.7,
            stl: 1.6,
            blk: 0.8,
            pf: 2.4,
            fga: 24.0,
            fta: 9.5,
        };
        let raw = raw_bpm(&s, 2.3, 1.0);
        // The page shows 18.7 before the constants and 15.6 after; each line is rounded.
        assert!((raw - 15.6).abs() < 0.2, "raw BPM {raw}");
        // The interpolated coefficients the page lists for position 2.3.
        assert!((BPM.ast.at(2.3) - 0.727).abs() < 0.001);
        assert!((BPM.orb.at(2.3) - 0.473).abs() < 0.001);
        assert!((BPM.stl.at(2.3) - 1.252).abs() < 0.001);
        assert!((BPM.blk.at(2.3) - 1.125).abs() < 0.001);
        // And his VORP: +7.6 BPM over 70% of the minutes of a full season.
        assert!((vorp(7.6, 0.70, 82) - 6.72).abs() < 1e-9);
        assert!((vorp(7.6, 0.70, 82) * WINS_PER_VORP - 18.1).abs() < 0.1);
    }

    #[test]
    fn listed_positions_parse_and_blend() {
        assert_eq!(listed_position("PG"), Some(1.0));
        assert_eq!(listed_position("C"), Some(5.0));
        assert_eq!(listed_position("G-F"), Some(2.5));
        assert_eq!(listed_position("F/C"), Some(4.25));
        assert_eq!(listed_position(""), None);
        assert_eq!(listed_position("coach"), None);
    }

    #[test]
    fn centring_puts_the_minutes_weighted_mean_at_three_within_the_limits() {
        let mut v = vec![0.0, 1.0, 2.0, 9.0];
        let w = vec![1.0, 1.0, 1.0, 1.0];
        centre_on_three(&mut v, &w);
        let mean: f64 = v.iter().sum::<f64>() / 4.0;
        assert!((mean - 3.0).abs() < 1e-6, "{v:?}");
        assert!(v.iter().all(|x| (1.0..=5.0).contains(x)));
        assert_eq!(v[3], 5.0);
    }

    #[test]
    fn possessions_follow_the_reference_formula() {
        let team = Totals {
            fga: 90.0,
            fgm: 40.0,
            fta: 20.0,
            oreb: 10.0,
            dreb: 30.0,
            tov: 15.0,
            ..Default::default()
        };
        let cov = Coverage {
            minutes: true,
            rebound_split: true,
            steals: true,
            blocks: true,
            turnovers: true,
        };
        let own = 90.0 + 0.4 * 20.0 - 1.07 * (10.0 / 40.0) * 50.0 + 15.0;
        assert!((possessions(&team, &team, cov) - own).abs() < 1e-9);
    }

    #[test]
    fn team_sums_match_the_adjusted_rating_and_the_elo_identity_holds() {
        let (games, all) = season();
        let t = table(SeasonYear::of(2025), &games, &all, &HashMap::new());
        assert!(t.bpm_available, "{:?}", t.note);
        assert_eq!(t.games, 12);
        assert_eq!(t.teams.len(), 4);
        assert_eq!(t.players.len(), 40);
        for team in &t.teams {
            let rows: Vec<&PlayerBox> = t
                .players
                .iter()
                .filter(|p| p.team_id == team.team_id)
                .collect();
            let sum: f64 = rows.iter().map(|p| p.bpm * p.minutes_share).sum();
            assert!(
                (sum - team.adjusted_rating).abs() < 1e-6,
                "team {:?}: {sum} vs {}",
                team.team_id,
                team.adjusted_rating
            );
            let shares: f64 = rows.iter().map(|p| p.minutes_share).sum();
            assert!((shares - 5.0).abs() < 1e-9);
            // Worths sum to the team's margin over a replacement team, per game, in Elo.
            // Nobody missed a game here, so the per-game-played worths sum to the team's
            // margin over a replacement team, per game, in Elo; the VORP shares always do.
            let worth: f64 = rows.iter().map(|p| p.worth_elo).sum();
            let pace = team.possessions / f64::from(team.games);
            let expected =
                (team.adjusted_rating - 5.0 * REPLACEMENT_LEVEL) * pace / 100.0 * ELO_PER_POINT;
            assert!((worth - expected).abs() < 1e-6);
            let vorp_sum: f64 = rows.iter().map(|p| p.vorp).sum();
            let expected_vorp =
                (team.adjusted_rating - 5.0 * REPLACEMENT_LEVEL) * f64::from(team.games) / 82.0;
            assert!((vorp_sum - expected_vorp).abs() < 1e-6);
            let osum: f64 = rows.iter().map(|p| p.obpm * p.minutes_share).sum();
            let otarget = team.offensive_rating + LEAD_EFFECT / 4.0 * team.average_lead;
            assert!((osum - otarget).abs() < 1e-6, "{osum} vs {otarget}");
            let mean_pos: f64 = rows
                .iter()
                .map(|p| p.position * p.minutes_share)
                .sum::<f64>()
                / 5.0;
            assert!((mean_pos - 3.0).abs() < 1e-6, "{mean_pos}");
        }
        // The strong team rates above the weak one, and its edge player leads the league.
        let rating = |id: i32| {
            t.teams
                .iter()
                .find(|x| x.team_id == TeamId::new(id))
                .unwrap()
                .rating
        };
        assert!(rating(1) > rating(2) && rating(2) > rating(4));
        assert_eq!(t.players[0].player_id, PlayerId::new(100));
        assert!(t.players[0].vorp > 0.0);
        // The player who takes the team's assists is estimated a creator, the blocker a big.
        let by = |id: i32| {
            t.players
                .iter()
                .find(|p| p.player_id == PlayerId::new(id))
                .unwrap()
        };
        assert!(by(200).offensive_role < by(202).offensive_role);
        assert!(by(204).position > by(200).position);
        // PER is scaled so the league's minutes-weighted mean is 15.
        let minutes: f64 = t.players.iter().map(|p| p.minutes).sum();
        let mean_per: f64 = t
            .players
            .iter()
            .map(|p| p.per.unwrap() * p.minutes)
            .sum::<f64>()
            / minutes;
        assert!((mean_per - 15.0).abs() < 1e-6, "{mean_per}");
        assert!(!t.per_partial);
    }

    #[test]
    fn a_season_without_steals_has_per_but_no_bpm_and_one_without_minutes_has_nothing() {
        let (games, mut all) = season();
        for l in &mut all {
            l.stl = None;
            l.blk = None;
        }
        let t = table(SeasonYear::of(2025), &games, &all, &HashMap::new());
        assert!(!t.bpm_available);
        assert!(t.per_available && t.per_partial);
        assert!(t.players.iter().all(|p| p.vorp == 0.0 && p.per.is_some()));
        assert!(t.note.as_deref().unwrap().contains("PER"));
        // Sorted by PER when BPM is unavailable.
        assert!(t
            .players
            .windows(2)
            .all(|w| w[0].per.unwrap() >= w[1].per.unwrap()));
        for l in &mut all {
            l.seconds = None;
        }
        let t = table(SeasonYear::of(2025), &games, &all, &HashMap::new());
        assert!(t.players.is_empty());
        assert!(t.note.as_deref().unwrap().contains("minutes"));
    }

    #[test]
    fn missing_turnovers_are_imputed_and_said() {
        let (games, mut all) = season();
        for l in &mut all {
            l.tov = None;
        }
        let t = table(SeasonYear::of(1975), &games, &all, &HashMap::new());
        assert!(t.bpm_available);
        assert!(t.players.iter().all(|p| p.turnovers_imputed));
        assert!(t.note.as_deref().unwrap().contains("imputed"));
        // By hand: 4.1119 - 0.0191*27 - 0.0519*75 - 0.0494*20 + 0.0442*8 + 0.0957*16
        // + 0.2056*4 + 0.2216*4 + 0.1972*3.
        let imputed = imputed_turnovers(SeasonYear::of(1975), 20.0, 8.0, 16.0, 4.0, 4.0, 3.0);
        assert!((imputed - 2.9009).abs() < 1e-4, "{imputed}");
    }

    #[test]
    fn a_listed_position_pulls_a_low_minutes_estimate_toward_it() {
        let (games, all) = season();
        let plain = table(SeasonYear::of(2025), &games, &all, &HashMap::new());
        let mut listed = HashMap::new();
        // Reserve 107 plays 12 minutes a game: 50 minutes of "C" weigh against 72 minutes.
        listed.insert(PlayerId::new(107), 5.0);
        let with = table(SeasonYear::of(2025), &games, &all, &listed);
        let pos = |t: &BoxTable, id: i32| {
            t.players
                .iter()
                .find(|p| p.player_id == PlayerId::new(id))
                .unwrap()
                .position
        };
        assert!(
            pos(&with, 107) > pos(&plain, 107) + 0.3,
            "{} vs {}",
            pos(&with, 107),
            pos(&plain, 107)
        );
        // The team is still centred on 3.
        let team1: Vec<&PlayerBox> = with
            .players
            .iter()
            .filter(|p| p.team_id == TeamId::new(1))
            .collect();
        let mean: f64 = team1
            .iter()
            .map(|p| p.position * p.minutes_share)
            .sum::<f64>()
            / 5.0;
        assert!((mean - 3.0).abs() < 1e-6);
    }

    #[test]
    fn a_traded_player_has_one_row_per_team_and_his_worth_is_per_game_played() {
        let (games, mut all) = season();
        // Player 100 leaves team 1 for team 2 after team 1's first three games; in the games
        // he misses with either team he simply has no line.
        let team1_games: Vec<i64> = games
            .iter()
            .filter(|g| g.home_team_id == TeamId::new(1) || g.away_team_id == TeamId::new(1))
            .map(|g| g.id)
            .collect();
        let team2_games: Vec<i64> = games
            .iter()
            .filter(|g| g.home_team_id == TeamId::new(2) || g.away_team_id == TeamId::new(2))
            .map(|g| g.id)
            .collect();
        all.retain(|l| {
            !(l.player_id == PlayerId::new(100) && team1_games[3..].contains(&l.game_id))
        });
        // Team-mates absorb his minutes either way, so both teams keep 240 minutes a game.
        for l in all.iter_mut() {
            if l.player_id == PlayerId::new(101) && team1_games[3..].contains(&l.game_id) {
                l.seconds = Some(l.seconds.unwrap() + 36 * 60);
            }
            if l.player_id == PlayerId::new(201) && team2_games[3..].contains(&l.game_id) {
                l.seconds = Some(l.seconds.unwrap() - 36 * 60);
            }
        }
        for gid in &team2_games[3..] {
            all.push(BoxLine {
                game_id: *gid,
                player_id: PlayerId::new(100),
                team_id: TeamId::new(2),
                seconds: Some(36 * 60),
                pts: Some(16),
                reb: Some(3),
                oreb: Some(0),
                dreb: Some(3),
                ast: Some(9),
                stl: Some(1),
                blk: Some(0),
                tov: Some(1),
                pf: Some(2),
                fgm: Some(6),
                fga: Some(12),
                fg3m: Some(0),
                ftm: Some(4),
                fta: Some(4),
            });
        }
        let t = table(SeasonYear::of(2025), &games, &all, &HashMap::new());
        let rows: Vec<&PlayerBox> = t
            .players
            .iter()
            .filter(|p| p.player_id == PlayerId::new(100))
            .collect();
        assert_eq!(rows.len(), 2);
        for r in &rows {
            assert_eq!(r.games, 3);
            // Half the team's games, so half the season share, but a full share when playing.
            assert!(
                (r.minutes_share - 0.375).abs() < 1e-9,
                "{}",
                r.minutes_share
            );
            let team = t.teams.iter().find(|x| x.team_id == r.team_id).unwrap();
            let pace = team.possessions / f64::from(team.games);
            let expected = (r.bpm - REPLACEMENT_LEVEL) * 0.75 * pace / 100.0;
            assert!((r.worth_points - expected).abs() < 1e-9);
            assert!((r.worth_elo - expected * ELO_PER_POINT).abs() < 1e-9);
        }
    }

    #[test]
    fn a_team_whose_lines_carry_no_minutes_is_skipped_and_counted() {
        let (games, mut all) = season();
        for l in all.iter_mut().filter(|l| l.team_id == TeamId::new(4)) {
            l.seconds = None;
        }
        let t = table(SeasonYear::of(2025), &games, &all, &HashMap::new());
        assert!(t.coverage.minutes);
        assert_eq!(t.teams_without_minutes, 1);
        assert!(t.players.iter().all(|p| p.team_id != TeamId::new(4)));
        assert!(t.note.as_deref().unwrap().contains("without minutes"));
        // The other teams still sum to their adjusted ratings.
        let team1 = t
            .teams
            .iter()
            .find(|x| x.team_id == TeamId::new(1))
            .unwrap();
        let sum: f64 = t
            .players
            .iter()
            .filter(|p| p.team_id == TeamId::new(1))
            .map(|p| p.bpm * p.minutes_share)
            .sum();
        assert!((sum - team1.adjusted_rating).abs() < 1e-6);
    }

    #[test]
    fn games_with_lines_for_one_team_only_are_left_out() {
        let (games, mut all) = season();
        all.retain(|l| !(l.game_id == 0 && l.team_id == TeamId::new(2)));
        let t = table(SeasonYear::of(2025), &games, &all, &HashMap::new());
        assert_eq!(t.games, 11);
        assert_eq!(t.games_without_lines, 1);
        let empty = table(SeasonYear::of(2025), &games, &[], &HashMap::new());
        assert!(empty.players.is_empty());
        assert_eq!(empty.games_without_lines, 12);
    }
}
