//! Player worth from absences, and the availability penalty of an injury report.
//!
//! The ratings know which team won. They do not know who played. A player's worth is what
//! his team loses when he sits: the regression of every game's residual (the actual margin
//! minus the margin the pre-game ratings expected) on the absences of both teams, with a
//! prior that pulls every worth toward zero. Two players out together are not each credited
//! with the whole gap, and a handful of missed games say little. The injury report of a day
//! then becomes a rating penalty per team and date for the games to come.

use std::collections::{BTreeMap, HashMap};

use chrono::{Duration, NaiveDate};
use serde::{Deserialize, Serialize};

use crate::domain::elo;
use crate::domain::ids::{PlayerId, SeasonYear, TeamId};
use crate::domain::models::Game;
use crate::domain::status::StatusTable;

/// Elo points per point of margin, the convention of the Cup margin model.
pub const ELO_PER_POINT: f64 = 28.0;
/// The prior spread of a player's worth when the data cannot say, in points per game:
/// about 84 Elo.
pub const DEFAULT_PRIOR_SD_POINTS: f64 = 3.0;
/// The prior spread estimated from the data stays within these bounds, in points.
pub const PRIOR_SD_BOUNDS: (f64, f64) = (1.0, 6.0);
/// A player needs this many absences to count toward the estimate of the prior spread.
pub const PRIOR_MIN_ABSENCES: u32 = 5;
/// The estimate of the prior spread needs this many such players; else the default holds.
pub const PRIOR_MIN_PLAYERS: usize = 10;
/// A player needs this many games played for a worth; fewer give no basis for his absence.
pub const MIN_GAMES_PLAYED: u32 = 5;
/// The largest penalty one absence may carry, in Elo points.
pub const MAX_ABSENCE_ELO: f64 = 400.0;
/// The most absences a forecast takes.
pub const MAX_ABSENCES: usize = 500;

#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct WorthParams {
    /// The prior standard deviation of a worth, in points per game; `None` estimates it
    /// from the spread of the raw with/without differences beyond their noise.
    #[serde(default)]
    pub prior_sd_points: Option<f64>,
}

/// One player's line in one game: on the team's box score, and whether he played.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Line {
    pub game_id: i64,
    pub player_id: PlayerId,
    pub team_id: TeamId,
    pub played: bool,
}

/// One player's worth in a season.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlayerWorth {
    pub player_id: PlayerId,
    /// The teams he played for in the season, in order of first appearance.
    pub teams: Vec<TeamId>,
    pub games_played: u32,
    /// Games of his team between his first and last appearance that he did not play.
    pub absences: u32,
    /// What the team loses per game without him, in points of margin.
    pub worth_points: f64,
    /// The same in Elo points.
    pub worth_elo: f64,
    pub se_points: f64,
    pub se_elo: f64,
    /// The team's mean residual with him and without him, before the regression.
    pub with_mean: f64,
    pub without_mean: f64,
}

/// The worth of every player with an absence in a season.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorthTable {
    pub season: SeasonYear,
    pub params: WorthParams,
    /// The prior spread the regression used, in points, and whether the data gave it.
    pub prior_sd_points: f64,
    pub prior_estimated: bool,
    /// Played games with both ratings and scores.
    pub games: usize,
    /// Games with at least one absence on either side.
    pub games_with_absence: usize,
    /// The standard deviation of the residuals, in points.
    pub residual_sd_points: f64,
    /// Players with lines in the season.
    pub players_seen: usize,
    /// By worth, highest first.
    pub players: Vec<PlayerWorth>,
}

/// The actual margin minus the expected one, from the home side, in points. The expected
/// margin follows the ratings' own home bonus, as the rating update does.
pub fn residual(g: &Game) -> Option<f64> {
    let (h, a) = (g.home_score?, g.away_score?);
    let bonus = if g.neutral {
        0.0
    } else {
        elo::DEFAULT_HOME_ADVANTAGE
    };
    let expected = (g.home_elo_pre + bonus - g.away_elo_pre) / ELO_PER_POINT;
    Some(f64::from(h - a) - expected)
}

/// An absence: a player who played for the team earlier and later in the season but not
/// in this game.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Absent {
    pub game_id: i64,
    pub player_id: PlayerId,
    pub team_id: TeamId,
}

/// A player's games played in the season and the teams he played for, in order of his
/// first game with each.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PlayerSummary {
    pub games_played: u32,
    pub teams: Vec<TeamId>,
}

/// The absences of a season and each player's summary.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Absences {
    pub absent: Vec<Absent>,
    pub players: HashMap<PlayerId, PlayerSummary>,
}

/// The absences of a season. A player is absent from a game of his team between his
/// first and last game for that team when he has no played line in it: a box-score line
/// with no minutes counts as an absence too. A line repeated for the same player and game
/// counts once.
pub fn absences(games: &[Game], lines: &[Line]) -> Absences {
    // Each team's played games in order.
    let mut order: Vec<&Game> = games.iter().filter(|g| g.is_played()).collect();
    order.sort_by_key(|g| (g.game_date, g.seq, g.id));
    let mut team_games: HashMap<TeamId, Vec<i64>> = HashMap::new();
    for g in &order {
        team_games.entry(g.home_team_id).or_default().push(g.id);
        team_games.entry(g.away_team_id).or_default().push(g.id);
    }
    let position: HashMap<(TeamId, i64), usize> = team_games
        .iter()
        .flat_map(|(&t, ids)| ids.iter().enumerate().map(move |(i, &g)| ((t, g), i)))
        .collect();
    let when: HashMap<i64, (NaiveDate, i16, i64)> = order
        .iter()
        .map(|g| (g.id, (g.game_date, g.seq, g.id)))
        .collect();
    // Each player's played games per team, as positions in the team's sequence.
    let mut played: BTreeMap<(PlayerId, TeamId), Vec<usize>> = BTreeMap::new();
    let mut seen: std::collections::HashSet<(PlayerId, TeamId, i64)> = Default::default();
    let mut players: HashMap<PlayerId, PlayerSummary> = HashMap::new();
    let mut first_seen: Vec<((NaiveDate, i16, i64), PlayerId, TeamId)> = Vec::new();
    for l in lines.iter().filter(|l| l.played) {
        let Some(&pos) = position.get(&(l.team_id, l.game_id)) else {
            continue;
        };
        if !seen.insert((l.player_id, l.team_id, l.game_id)) {
            continue;
        }
        played
            .entry((l.player_id, l.team_id))
            .or_default()
            .push(pos);
        let entry = players.entry(l.player_id).or_default();
        entry.games_played += 1;
        if !entry.teams.contains(&l.team_id) {
            first_seen.push((when[&l.game_id], l.player_id, l.team_id));
            entry.teams.push(l.team_id);
        }
    }
    first_seen.sort_unstable();
    for (_, p, t) in first_seen {
        let teams = &mut players.get_mut(&p).expect("seen").teams;
        teams.retain(|&x| x != t);
        teams.push(t);
    }
    let mut absent = Vec::new();
    for ((player_id, team_id), mut positions) in played {
        positions.sort_unstable();
        let (&first, &last) = (
            positions.first().expect("one"),
            positions.last().expect("one"),
        );
        let ids = &team_games[&team_id];
        let mut next = positions.iter().peekable();
        for (pos, &game_id) in ids.iter().enumerate().take(last + 1).skip(first) {
            if next.peek() == Some(&&pos) {
                next.next();
                continue;
            }
            absent.push(Absent {
                game_id,
                player_id,
                team_id,
            });
        }
    }
    absent.sort_by_key(|a| (a.game_id, a.player_id));
    Absences { absent, players }
}

/// The worth of every player with an absence, by ridge regression of the residuals on the
/// absences of both sides.
pub fn table(
    season: SeasonYear,
    games: &[Game],
    lines: &[Line],
    params: WorthParams,
) -> WorthTable {
    let Absences {
        absent,
        players: summary,
    } = absences(games, lines);
    let residuals: BTreeMap<i64, (f64, TeamId, TeamId)> = games
        .iter()
        .filter_map(|g| residual(g).map(|r| (g.id, (r, g.home_team_id, g.away_team_id))))
        .collect();
    let n_games = residuals.len();
    let residual_sd = {
        let rs: Vec<f64> = residuals.values().map(|v| v.0).collect();
        let mean = rs.iter().sum::<f64>() / rs.len().max(1) as f64;
        let var = rs.iter().map(|r| (r - mean).powi(2)).sum::<f64>()
            / rs.len().max(2).saturating_sub(1) as f64;
        var.sqrt()
    };
    // The players the regression estimates: an absence and enough games played.
    let mut players: Vec<PlayerId> = absent
        .iter()
        .map(|a| a.player_id)
        .filter(|p| {
            summary
                .get(p)
                .is_some_and(|s| s.games_played >= MIN_GAMES_PLAYED)
        })
        .collect();
    players.sort_unstable();
    players.dedup();
    let index: HashMap<PlayerId, usize> =
        players.iter().enumerate().map(|(i, &p)| (p, i)).collect();
    let m = players.len();
    // Rows: one per game with an absence; the sign says which side lost the player.
    let mut rows: BTreeMap<i64, Vec<(usize, f64)>> = BTreeMap::new();
    for a in &absent {
        let Some(&j) = index.get(&a.player_id) else {
            continue;
        };
        let Some(&(_, home, _)) = residuals.get(&a.game_id) else {
            continue;
        };
        let sign = if a.team_id == home { -1.0 } else { 1.0 };
        rows.entry(a.game_id).or_default().push((j, sign));
    }
    let games_with_absence = rows.len();
    // The plain with/without means, for the reader and for the prior.
    let mut sums: BTreeMap<PlayerId, ([f64; 2], [u32; 2])> = BTreeMap::new();
    for l in lines.iter().filter(|l| l.played) {
        if let Some(&(r, home, _)) = residuals.get(&l.game_id) {
            let signed = if l.team_id == home { r } else { -r };
            let e = sums.entry(l.player_id).or_default();
            e.0[0] += signed;
            e.1[0] += 1;
        }
    }
    for a_ in &absent {
        if let Some(&(r, home, _)) = residuals.get(&a_.game_id) {
            let signed = if a_.team_id == home { r } else { -r };
            let e = sums.entry(a_.player_id).or_default();
            e.0[1] += signed;
            e.1[1] += 1;
        }
    }
    // Normal equations with the ridge: (XᵀX + λI) β = Xᵀr, λ = σ² / τ².
    let sigma2 = residual_sd.powi(2).max(1.0);
    let (prior_sd, prior_estimated) = match params.prior_sd_points {
        Some(sd) => (sd.clamp(1e-3, 100.0), false),
        None => {
            let diffs: Vec<(f64, f64)> = sums
                .values()
                .filter(|(_, n)| n[0] >= MIN_GAMES_PLAYED && n[1] >= PRIOR_MIN_ABSENCES)
                .map(|(s, n)| {
                    let d = s[0] / f64::from(n[0]) - s[1] / f64::from(n[1]);
                    let noise = sigma2 * (1.0 / f64::from(n[0]) + 1.0 / f64::from(n[1]));
                    (d, noise)
                })
                .collect();
            match estimate_prior_sd(&diffs) {
                Some(sd) => (sd, true),
                None => (DEFAULT_PRIOR_SD_POINTS, false),
            }
        }
    };
    let tau2 = prior_sd.powi(2);
    let lambda = sigma2 / tau2;
    let mut a = vec![0.0; m * m];
    let mut b = vec![0.0; m];
    for (game_id, cols) in &rows {
        let r = residuals[game_id].0;
        for &(j, sj) in cols {
            b[j] += sj * r;
            for &(k, sk) in cols {
                a[j * m + k] += sj * sk;
            }
        }
    }
    for j in 0..m {
        a[j * m + j] += lambda;
    }
    let (beta, inv_diag) = solve_spd(&a, &b, m).unwrap_or((vec![0.0; m], vec![1.0 / lambda; m]));
    let absences_of: HashMap<PlayerId, u32> = absent.iter().fold(HashMap::new(), |mut acc, a_| {
        *acc.entry(a_.player_id).or_insert(0) += 1;
        acc
    });
    let mut out: Vec<PlayerWorth> = players
        .iter()
        .enumerate()
        .map(|(j, &p)| {
            let PlayerSummary {
                games_played,
                teams,
            } = summary.get(&p).cloned().unwrap_or_default();
            let (s, n) = sums.get(&p).copied().unwrap_or_default();
            let se = (sigma2 * inv_diag[j]).sqrt();
            PlayerWorth {
                player_id: p,
                teams,
                games_played,
                absences: absences_of.get(&p).copied().unwrap_or(0),
                worth_points: beta[j],
                worth_elo: beta[j] * ELO_PER_POINT,
                se_points: se,
                se_elo: se * ELO_PER_POINT,
                with_mean: if n[0] > 0 {
                    s[0] / f64::from(n[0])
                } else {
                    0.0
                },
                without_mean: if n[1] > 0 {
                    s[1] / f64::from(n[1])
                } else {
                    0.0
                },
            }
        })
        .collect();
    out.sort_by(|x, y| {
        y.worth_points
            .total_cmp(&x.worth_points)
            .then(x.player_id.cmp(&y.player_id))
    });
    WorthTable {
        season,
        params,
        prior_sd_points: prior_sd,
        prior_estimated,
        games: n_games,
        games_with_absence,
        residual_sd_points: residual_sd,
        players_seen: summary.len(),
        players: out,
    }
}

/// The spread of the true worths from the raw differences `(difference, noise variance)`:
/// the mean square of the differences less their mean noise, within `PRIOR_SD_BOUNDS`.
/// `None` with fewer than `PRIOR_MIN_PLAYERS` differences.
pub fn estimate_prior_sd(diffs: &[(f64, f64)]) -> Option<f64> {
    if diffs.len() < PRIOR_MIN_PLAYERS {
        return None;
    }
    let n = diffs.len() as f64;
    let mean_sq = diffs.iter().map(|(d, _)| d * d).sum::<f64>() / n;
    let mean_noise = diffs.iter().map(|(_, v)| v).sum::<f64>() / n;
    let tau2 = (mean_sq - mean_noise).max(0.0);
    Some(tau2.sqrt().clamp(PRIOR_SD_BOUNDS.0, PRIOR_SD_BOUNDS.1))
}

/// Solve A x = b for a symmetric positive definite A (row-major, m × m) by Cholesky, and
/// return x with the diagonal of A⁻¹. `None` when A is not positive definite.
fn solve_spd(a: &[f64], b: &[f64], m: usize) -> Option<(Vec<f64>, Vec<f64>)> {
    if m == 0 {
        return Some((vec![], vec![]));
    }
    let mut l = vec![0.0; m * m];
    for i in 0..m {
        for j in 0..=i {
            let mut s = a[i * m + j];
            for k in 0..j {
                s -= l[i * m + k] * l[j * m + k];
            }
            if i == j {
                if s <= 0.0 {
                    return None;
                }
                l[i * m + i] = s.sqrt();
            } else {
                l[i * m + j] = s / l[j * m + j];
            }
        }
    }
    let forward = |rhs: &[f64]| -> Vec<f64> {
        let mut y = vec![0.0; m];
        for i in 0..m {
            let mut s = rhs[i];
            for k in 0..i {
                s -= l[i * m + k] * y[k];
            }
            y[i] = s / l[i * m + i];
        }
        y
    };
    let y = forward(b);
    let mut x = vec![0.0; m];
    for i in (0..m).rev() {
        let mut s = y[i];
        for k in i + 1..m {
            s -= l[k * m + i] * x[k];
        }
        x[i] = s / l[i * m + i];
    }
    // (A⁻¹)ᵢᵢ = Σₖ (L⁻¹)ₖᵢ²: column i of L⁻¹ is the forward solve of the unit vector.
    let mut inv_diag = vec![0.0; m];
    let mut unit = vec![0.0; m];
    for (i, d) in inv_diag.iter_mut().enumerate() {
        unit.iter_mut().for_each(|v| *v = 0.0);
        unit[i] = 1.0;
        let col = forward(&unit);
        *d = col.iter().map(|v| v * v).sum();
    }
    Some((x, inv_diag))
}

// ---------------------------------------------------------------------------------------
// Availability
// ---------------------------------------------------------------------------------------

/// A status of the injury report, as the source names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AbsenceStatus {
    Out,
    OutForSeason,
    /// Not on the report, but a regular player who has not played his team's last games:
    /// read from the box scores, priced as out.
    Absent,
    Doubtful,
    Questionable,
    DayToDay,
    Probable,
    /// Listed as available: nothing comes off.
    Available,
    /// A text the application does not know: listed, not priced.
    Unknown,
}

impl AbsenceStatus {
    pub fn parse(status: &str) -> Self {
        let s = status.trim().to_ascii_lowercase();
        if s.contains("not available") {
            Self::Out
        } else if s.starts_with("available") || s.starts_with("active") || s.starts_with("healthy")
        {
            Self::Available
        } else if s.contains("season") {
            Self::OutForSeason
        } else if s.starts_with("out") || s.contains("suspen") || s.contains("not with team") {
            Self::Out
        } else if s.starts_with("doubtful") {
            Self::Doubtful
        } else if s.starts_with("questionable") || s.contains("game time") {
            Self::Questionable
        } else if s.contains("day") {
            Self::DayToDay
        } else if s.starts_with("probable") {
            Self::Probable
        } else {
            Self::Unknown
        }
    }

    /// The status of a report line: the status text, and the description when it says the
    /// player is done for the season while the status does not.
    pub fn parse_line(status: &str, description: Option<&str>) -> Self {
        let parsed = Self::parse(status);
        let done = description.is_some_and(|d| {
            let d = d.to_ascii_lowercase();
            // "For the season opener" and "avoided a season-ending injury" say the opposite.
            let about_a_game = ["opener", "debut", "finale", "start of the season"]
                .iter()
                .any(|k| d.contains(k));
            let denied = ["no season-ending", "not season-ending", "avoid"]
                .iter()
                .any(|k| d.contains(k));
            let ends = [
                "out for the season",
                "out for season",
                "done for the season",
                "miss the rest of the season",
                "miss the remainder of the season",
                "out for the rest of the season",
                "out for the remainder of the season",
                "season-ending",
                "season ending",
            ]
            .iter()
            .any(|k| d.contains(k));
            ends && !about_a_game && !denied
        });
        if done && parsed.weight() > 0.0 {
            Self::OutForSeason
        } else {
            parsed
        }
    }

    /// The share of the player's worth the status removes.
    pub fn weight(self) -> f64 {
        match self {
            Self::Out | Self::OutForSeason | Self::Absent => 1.0,
            Self::Doubtful => 0.75,
            Self::Questionable | Self::DayToDay => 0.5,
            Self::Probable => 0.25,
            Self::Available | Self::Unknown => 0.0,
        }
    }

    /// How many days the absence lasts from the report when no return date is given.
    pub fn default_days(self) -> i64 {
        match self {
            Self::OutForSeason => 366,
            Self::Out | Self::Absent => 14,
            Self::Doubtful | Self::DayToDay => 3,
            Self::Questionable => 2,
            Self::Probable => 1,
            Self::Available | Self::Unknown => 0,
        }
    }
}

/// A rating penalty for a team over a span of dates: a player out, weighted by his status.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Absence {
    pub team_id: TeamId,
    pub from: NaiveDate,
    /// Inclusive.
    pub to: NaiveDate,
    /// The Elo points taken off the team's rating for the games in the span.
    pub elo: f64,
    #[serde(default)]
    pub player_id: Option<PlayerId>,
    /// The standard error of the penalty. Above zero, the season engine draws the penalty
    /// of each simulated season from it; at zero the penalty is fixed.
    #[serde(default)]
    pub se_elo: f64,
}

impl Absence {
    pub fn covers(&self, date: NaiveDate) -> bool {
        self.from <= date && date <= self.to
    }
}

/// One injury-report line with the player's worth resolved.
#[derive(Debug, Clone, PartialEq)]
pub struct ReportedAbsence {
    pub player_id: PlayerId,
    pub team_id: TeamId,
    pub status: AbsenceStatus,
    pub return_date: Option<NaiveDate>,
    pub worth_elo: f64,
    /// The standard error of the worth, in Elo; zero when the estimate has none.
    pub se_elo: f64,
    /// Team games the player has missed in a row up to the latest game with box scores
    /// (zero when he played it); none when the team has no box scores to read.
    pub missed_so_far: Option<u32>,
}

/// A player in a run of this many missed games is in a spell whatever his status says.
pub const SPELL_MIN_MISSED: u32 = 3;

/// What set the end of a priced absence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpanSource {
    /// The report's return date, which lies beyond the team's next game.
    ReturnDate,
    /// The expected remaining games of a run this long, from the survival curve.
    Expected,
    /// The games a line of this status has gone on to miss, from the status curve.
    Measured,
    /// The status's default span.
    Status,
    /// Out for the season.
    Season,
}

/// What set the share of the worth an absence takes off.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WeightSource {
    /// The share of the lines of this status, in this state, whose player missed the next
    /// game: the status curve.
    Measured,
    /// The status's default weight.
    Status,
}

/// A priced absence's span and weight, with what set each.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Span {
    /// Inclusive.
    pub to: NaiveDate,
    pub source: SpanSource,
    /// The share of the worth taken off over the span.
    pub weight: f64,
    pub weight_source: WeightSource,
}

/// What the game logs add to the pricing of a report: the survival curve of the runs of
/// missed games, the curves of what each status has meant, and each team's unplayed games
/// from the report date on, in order. Either curve is absent below its minimum of data.
pub struct SpanRule<'a> {
    pub survival: Option<&'a crate::domain::spell::Survival>,
    pub statuses: Option<&'a StatusTable>,
    pub upcoming: &'a HashMap<TeamId, Vec<NaiveDate>>,
}

/// When a line's absence ends (inclusive), what share of the worth it takes off, and what
/// decided each; none when nothing is priced.
///
/// The weight is the status curve's, for the player's state (not yet out, or in a run of
/// one game or more), when the stored reports back a curve for the status; else the
/// status's default. Out for the season keeps its full weight, and so does a line whose
/// return date stands, the source's own estimate. A status measured as never missing the
/// next game prices nothing. A rule without either curve is no rule.
///
/// The span: out for the season ends with the season. A return date beyond the team's
/// next game is the source's estimate and stands. A return date that is the next game or
/// earlier is the feed's placeholder. A player listed out, or in a run of
/// [`SPELL_MIN_MISSED`] games, is then priced for the expected remaining games of a run as
/// long as his, read off the survival curve, when the games he has missed so far are
/// known. Any other line with a status curve is priced for the games a line of its status
/// has gone on to miss. Both are laid over the team's schedule. The rest, every line
/// without a rule and a team without box scores take the status's default span. Without a
/// curve to lean on, a return date of the report day or before prices nothing.
pub fn span_for(
    r: &ReportedAbsence,
    report_date: NaiveDate,
    season_end: NaiveDate,
    rule: Option<&SpanRule>,
) -> Option<Span> {
    if r.status.weight() <= 0.0 {
        return None;
    }
    // A rule without a curve adds nothing: the line is priced as without one.
    let rule = rule.filter(|x| x.survival.is_some() || x.statuses.is_some());
    let upcoming = rule.and_then(|x| x.upcoming.get(&r.team_id));
    // The status curve in the player's state, when the stored reports back one and the
    // state is known.
    let curve = rule
        .and_then(|x| x.statuses)
        .filter(|_| r.status != AbsenceStatus::OutForSeason)
        .and_then(|t| r.missed_so_far.and_then(|k| t.curve(r.status, k >= 1)));
    let (weight, weight_source) = match curve {
        Some(c) => (c.weight(), WeightSource::Measured),
        None => (r.status.weight(), WeightSource::Status),
    };
    // Measured as never missing the next game: nothing to price.
    if weight <= 0.0 {
        return None;
    }
    let span_at = |to: NaiveDate, source, weight, weight_source| {
        Some(Span {
            to: to.min(season_end),
            source,
            weight,
            weight_source,
        })
    };
    let span = |to: NaiveDate, source| span_at(to, source, weight, weight_source);
    if r.status == AbsenceStatus::OutForSeason {
        return span(season_end, SpanSource::Season);
    }
    let next_game = upcoming.and_then(|u| u.first().copied());
    // In a spell: listed out or absent, or demonstrably out for a run of games. The
    // expected rule needs the games missed so far to be known.
    let in_spell = (matches!(r.status, AbsenceStatus::Out | AbsenceStatus::Absent)
        || r.missed_so_far.is_some_and(|k| k >= SPELL_MIN_MISSED))
        && r.missed_so_far.is_some();
    let survival = rule.and_then(|x| x.survival);
    let expected_applies = in_spell && survival.is_some() && upcoming.is_some();
    let measured_applies = curve.is_some() && upcoming.is_some();
    if let Some(d) = r.return_date {
        let placeholder = next_game.is_some_and(|g| d <= g);
        // With a curve to lean on, a date up to the next game, today's date included, is
        // the feed's placeholder. Otherwise a date of today or before prices nothing, and
        // a later one stands.
        if d <= report_date && !(placeholder && (expected_applies || measured_applies)) {
            return None;
        }
        if !placeholder {
            // The source's own estimate: priced at the status's default weight, since the
            // curve's weight is measured over every line of the status, dated or not.
            return span_at(
                d - Duration::days(1),
                SpanSource::ReturnDate,
                r.status.weight(),
                WeightSource::Status,
            );
        }
    }
    if let (Some(survival), Some(upcoming), true, Some(missed)) =
        (survival, upcoming, in_spell, r.missed_so_far)
    {
        let games = survival.expected_remaining(missed).round().max(1.0) as usize;
        let to = upcoming.get(games - 1).copied().unwrap_or(season_end);
        return span(to, SpanSource::Expected);
    }
    if let (Some(c), Some(upcoming)) = (curve, upcoming) {
        let games = c.span_games().round().max(1.0) as usize;
        let to = upcoming.get(games - 1).copied().unwrap_or(season_end);
        return span(to, SpanSource::Measured);
    }
    let to = report_date + Duration::days(r.status.default_days() - 1);
    span(to, SpanSource::Status)
}

/// The penalties of a report dated `report_date`, each line's span and weight from
/// [`span_for`], never past `season_end`. A worth at or below zero gives no penalty: the model does not
/// reward an absence. A status that removes nothing (available, unknown) gives none
/// either.
pub fn absences_from_report(
    reported: &[ReportedAbsence],
    report_date: NaiveDate,
    season_end: NaiveDate,
    rule: Option<&SpanRule>,
) -> Vec<Absence> {
    reported
        .iter()
        .filter(|r| r.worth_elo > 0.0)
        .filter_map(|r| {
            let span = span_for(r, report_date, season_end, rule)?;
            if span.to < report_date {
                return None;
            }
            let elo = (r.worth_elo * span.weight).min(MAX_ABSENCE_ELO);
            let se_elo = (r.se_elo.max(0.0) * span.weight).min(MAX_ABSENCE_ELO);
            let to = span.to;
            (elo > 0.0).then_some(Absence {
                team_id: r.team_id,
                from: report_date,
                to,
                elo,
                player_id: Some(r.player_id),
                se_elo,
            })
        })
        .collect()
}

/// What a set of absences costs a team over its remaining regular-season games, priced
/// game by game at today's ratings, with the opponents at full strength.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AbsenceEffect {
    /// Regular-season games on the schedule without a result, dated `today` or later.
    pub games: u32,
    /// Those with a penalty.
    pub games_covered: u32,
    /// The penalties summed over the games covered, in Elo times games.
    pub elo_games: f64,
    /// The mean penalty over the remaining games: `elo_games` over `games`.
    pub mean_penalty: f64,
    /// The win probability at the full rating less the probability at the penalized one,
    /// summed over the games: expected wins lost, to first order (the ratings do not move).
    pub wins_lost: f64,
}

/// The effect of `absences` on `team` over its remaining games. `ratings` holds every
/// team's current rating; an opponent without one is read at the league mean. The home
/// bonus is the season's regular-season one.
pub fn absence_effect(
    absences: &[Absence],
    games: &[Game],
    ratings: &HashMap<TeamId, f64>,
    team: TeamId,
    today: NaiveDate,
    home_advantage: f64,
) -> AbsenceEffect {
    let rating = ratings.get(&team).copied().unwrap_or(elo::SEASON_MEAN);
    let mut effect = AbsenceEffect {
        games: 0,
        games_covered: 0,
        elo_games: 0.0,
        mean_penalty: 0.0,
        wins_lost: 0.0,
    };
    for g in games.iter().filter(|g| {
        g.game_type == crate::domain::models::GameType::Regular
            && !g.is_played()
            && g.game_date >= today
            && g.involves(team)
    }) {
        effect.games += 1;
        let p = penalty(absences, team, g.game_date);
        if p <= 0.0 {
            continue;
        }
        effect.games_covered += 1;
        effect.elo_games += p;
        let home = g.home_team_id == team;
        let opponent = if home { g.away_team_id } else { g.home_team_id };
        let other = ratings.get(&opponent).copied().unwrap_or(elo::SEASON_MEAN);
        let full = elo::game_probability(rating, other, home, g.neutral, home_advantage, 1.0);
        let down = elo::game_probability(rating - p, other, home, g.neutral, home_advantage, 1.0);
        effect.wins_lost += full - down;
    }
    if effect.games > 0 {
        effect.mean_penalty = effect.elo_games / f64::from(effect.games);
    }
    effect
}

/// One game of the next night, priced at the ratings both teams play at that night.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NightGame {
    pub game_id: i64,
    pub home_team_id: TeamId,
    pub away_team_id: TeamId,
    pub neutral: bool,
    /// False when the team had no rating and the league mean stands in.
    pub home_rated: bool,
    pub away_rated: bool,
    /// The ratings as of the night, the penalties on the night's date, and the ratings
    /// the teams play at.
    pub home_rating: f64,
    pub away_rating: f64,
    pub home_penalty: f64,
    pub away_penalty: f64,
    pub home_tonight: f64,
    pub away_tonight: f64,
    /// The home team's win probability at full strength and at the penalized ratings.
    pub p_home_full: f64,
    pub p_home_tonight: f64,
}

/// The next night with regular-season games still to play.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Night {
    pub date: NaiveDate,
    /// The night's regular-season games already played, when the night is today and
    /// partly over.
    pub played: u32,
    /// The games still to play, priced.
    pub games: Vec<NightGame>,
}

/// The next night with regular-season games: the earliest date on or after `as_of` with
/// an unplayed regular-season game, and its unplayed games priced at the ratings the
/// teams play at. None when no such game remains. A team without a rating is read at the
/// league mean, and `NightGame` says so.
pub fn tonight(
    games: &[Game],
    as_of: NaiveDate,
    ratings: &HashMap<TeamId, f64>,
    absences: &[Absence],
    home_advantage: f64,
) -> Option<Night> {
    let regular = |g: &&Game| g.game_type == crate::domain::models::GameType::Regular;
    let unplayed = |g: &&Game| regular(g) && !g.is_played() && g.game_date >= as_of;
    let night = games.iter().filter(unplayed).map(|g| g.game_date).min()?;
    let played = games
        .iter()
        .filter(regular)
        .filter(|g| g.is_played() && g.game_date == night)
        .count() as u32;
    let mut slate: Vec<&Game> = games
        .iter()
        .filter(unplayed)
        .filter(|g| g.game_date == night)
        .collect();
    slate.sort_by_key(|g| (g.seq, g.id));
    let rating = |t: TeamId| ratings.get(&t).copied().unwrap_or(elo::SEASON_MEAN);
    let priced = slate
        .into_iter()
        .map(|g| {
            let home_rating = rating(g.home_team_id);
            let away_rating = rating(g.away_team_id);
            let home_penalty = penalty(absences, g.home_team_id, night);
            let away_penalty = penalty(absences, g.away_team_id, night);
            let p =
                |h: f64, a: f64| elo::game_probability(h, a, true, g.neutral, home_advantage, 1.0);
            NightGame {
                game_id: g.id,
                home_team_id: g.home_team_id,
                away_team_id: g.away_team_id,
                neutral: g.neutral,
                home_rated: ratings.contains_key(&g.home_team_id),
                away_rated: ratings.contains_key(&g.away_team_id),
                home_rating,
                away_rating,
                home_penalty,
                away_penalty,
                home_tonight: home_rating - home_penalty,
                away_tonight: away_rating - away_penalty,
                p_home_full: p(home_rating, away_rating),
                p_home_tonight: p(home_rating - home_penalty, away_rating - away_penalty),
            }
        })
        .collect();
    Some(Night {
        date: night,
        played,
        games: priced,
    })
}

/// The absences that cover `as_of`, each held to `until` (inclusive) when it ends before
/// that: the roster of `as_of` kept to the season's end.
pub fn held(absences: &[Absence], as_of: NaiveDate, until: NaiveDate) -> Vec<Absence> {
    absences
        .iter()
        .filter(|a| a.covers(as_of))
        .map(|a| Absence {
            to: a.to.max(until),
            ..a.clone()
        })
        .collect()
}

/// The penalty of `team` on `date`: the sum of its absences that cover the date. An
/// absence with a standard error counts its expected drawn penalty, see
/// [`expected_penalty`].
pub fn penalty(absences: &[Absence], team: TeamId, date: NaiveDate) -> f64 {
    // A fold from zero: an empty sum of floats is negative zero, which serializes as -0.0.
    absences
        .iter()
        .filter(|a| a.team_id == team && a.covers(date))
        .map(|a| expected_penalty(a.elo, a.se_elo))
        .fold(0.0, |acc, p| acc + p)
}

/// The mean of a penalty drawn from a normal distribution around `elo` with standard error
/// `se`, held between zero and the cap: what the engine applies on average. A small
/// estimate with a large error is pulled upward, because the draw stops at zero. With no
/// error it is `elo` itself.
pub fn expected_penalty(elo: f64, se: f64) -> f64 {
    if se <= 0.0 {
        return elo.clamp(0.0, MAX_ABSENCE_ELO);
    }
    let a = (0.0 - elo) / se;
    let b = (MAX_ABSENCE_ELO - elo) / se;
    elo * (normal_cdf(b) - normal_cdf(a))
        + se * (normal_pdf(a) - normal_pdf(b))
        + MAX_ABSENCE_ELO * (1.0 - normal_cdf(b))
}

fn normal_pdf(x: f64) -> f64 {
    (-0.5 * x * x).exp() / (2.0 * std::f64::consts::PI).sqrt()
}

/// The standard normal distribution function, by Abramowitz and Stegun 7.1.26 (error
/// below 1.5e-7).
fn normal_cdf(x: f64) -> f64 {
    let z = x / std::f64::consts::SQRT_2;
    let t = 1.0 / (1.0 + 0.3275911 * z.abs());
    let poly = t
        * (0.254829592
            + t * (-0.284496736 + t * (1.421413741 + t * (-1.453152027 + t * 1.061405429))));
    let erf = 1.0 - poly * (-z * z).exp();
    0.5 * (1.0 + if z < 0.0 { -erf } else { erf })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::models::{EloSource, GameType, ResultSource};

    fn d(day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2025, 1, 1).unwrap() + Duration::days(i64::from(day))
    }

    fn game(id: i64, day: u32, home: i32, away: i32, margin: i32) -> Game {
        Game {
            id,
            season: SeasonYear::of(2025),
            game_date: d(day),
            seq: 0,
            home_team_id: TeamId::new(home),
            away_team_id: TeamId::new(away),
            neutral: false,
            game_type: GameType::Regular,
            home_score: Some(100 + margin),
            away_score: Some(100),
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

    /// A small deterministic generator for the noise.
    struct Lcg(u64);
    impl Lcg {
        fn next_f64(&mut self) -> f64 {
            self.0 = self
                .0
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            ((self.0 >> 11) as f64) / ((1u64 << 53) as f64)
        }
        /// Roughly normal: the sum of twelve uniforms minus six.
        fn normal(&mut self) -> f64 {
            (0..12).map(|_| self.next_f64()).sum::<f64>() - 6.0
        }
    }

    /// Four teams, a round robin repeated; player 1 (team 1) is worth 6 points and sits
    /// out a block of games; player 2 (team 2) is worth 3 and sits a different block.
    fn season(absent_games: &dyn Fn(PlayerId, usize) -> bool) -> (Vec<Game>, Vec<Line>) {
        let mut rng = Lcg(7);
        let worth = |p: PlayerId| match p.as_i32() {
            1 => 6.0,
            2 => 3.0,
            _ => 0.0,
        };
        let mut games = Vec::new();
        let mut lines = Vec::new();
        let mut id = 0;
        let pairs = [(1, 2), (1, 3), (1, 4), (2, 3), (2, 4), (3, 4)];
        for round in 0..30 {
            for (k, &(a, b)) in pairs.iter().enumerate() {
                let (home, away) = if round % 2 == 0 { (a, b) } else { (b, a) };
                let n = round * pairs.len() + k;
                id += 1;
                let mut expected_margin = 100.0 / ELO_PER_POINT;
                // Every team has three rostered players: 10 × team + 1..3; player 1 and 2
                // are the stars of teams 1 and 2.
                for t in [home, away] {
                    for slot in 1..=3 {
                        let p = PlayerId::new(if slot == 1 && t <= 2 {
                            t
                        } else {
                            10 * t + slot
                        });
                        let played = !absent_games(p, n);
                        lines.push(Line {
                            game_id: id,
                            player_id: p,
                            team_id: TeamId::new(t),
                            played,
                        });
                        if !played {
                            let sign = if t == home { -1.0 } else { 1.0 };
                            expected_margin += sign * worth(p);
                        }
                    }
                }
                let margin = (expected_margin + 12.0 * rng.normal()).round() as i32;
                games.push(game(id, n as u32, home, away, margin));
            }
        }
        (games, lines)
    }

    #[test]
    fn residual_is_the_margin_minus_the_rating_expectation() {
        let g = game(1, 0, 1, 2, 10);
        // Equal ratings at home: the expectation is 100 / 28 points.
        assert!((residual(&g).unwrap() - (10.0 - 100.0 / 28.0)).abs() < 1e-9);
        let mut scheduled = g.clone();
        scheduled.home_score = None;
        assert_eq!(residual(&scheduled), None);
    }

    #[test]
    fn absences_lie_between_the_first_and_last_appearance() {
        let (games, mut lines) = season(&|p, n| p == PlayerId::new(1) && (20..30).contains(&n));
        // Player 1 also has no line at all (not even a DNP) in his team's games 40-42,
        // and no lines before game 2: only the gap inside counts.
        lines.retain(|l| {
            !(l.player_id == PlayerId::new(1)
                && ((40..43).contains(&(l.game_id as usize - 1)) || l.game_id <= 2))
        });
        let Absences {
            absent,
            players: summary,
        } = absences(&games, &lines);
        let p1: Vec<&Absent> = absent
            .iter()
            .filter(|a| a.player_id == PlayerId::new(1))
            .collect();
        // Team 1 plays in games 20..30 three times per round of six: its games in that
        // span are the ones with team 1, plus the three games 40..42 it plays or not.
        let team1_games_in = |lo: usize, hi: usize| {
            games
                .iter()
                .filter(|g| {
                    (lo..hi).contains(&(g.id as usize - 1))
                        && (g.home_team_id == TeamId::new(1) || g.away_team_id == TeamId::new(1))
                })
                .count()
        };
        assert_eq!(p1.len(), team1_games_in(20, 30) + team1_games_in(40, 43));
        assert!(p1.iter().all(|a| a.team_id == TeamId::new(1)));
        let PlayerSummary {
            games_played,
            teams,
        } = &summary[&PlayerId::new(1)];
        assert_eq!(*teams, vec![TeamId::new(1)]);
        assert_eq!(*games_played as usize, 90 - p1.len() - team1_games_in(0, 2));
        // A repeated line counts once.
        let mut twice = lines.clone();
        twice.push(lines[0]);
        assert_eq!(absences(&games, &twice), absences(&games, &lines));
        // A player with no absence has none.
        assert!(absent.iter().all(|a| a.player_id != PlayerId::new(33)));
    }

    const PRIOR: WorthParams = WorthParams {
        prior_sd_points: Some(4.0),
    };

    #[test]
    fn the_regression_recovers_the_worth_and_shrinks_the_rare_absence() {
        // The star sits 120 of the 180 games (60 of his team's), the second star 40 of his.
        let (games, lines) = season(&|p, n| {
            (p == PlayerId::new(1) && (20..140).contains(&n))
                || (p == PlayerId::new(2) && (40..120).contains(&n))
                || (p == PlayerId::new(33) && n == 51)
        });
        let t = table(SeasonYear::of(2025), &games, &lines, PRIOR);
        assert_eq!(t.games, 180);
        assert!(!t.prior_estimated && t.prior_sd_points == 4.0);
        assert!(
            t.residual_sd_points > 10.0 && t.residual_sd_points < 15.0,
            "{}",
            t.residual_sd_points
        );
        let of = |p: i32| {
            t.players
                .iter()
                .find(|w| w.player_id == PlayerId::new(p))
                .unwrap()
        };
        let star = of(1);
        assert_eq!(star.absences, 60);
        // With λ = σ²/τ² and 60 absences the posterior keeps n / (n + λ) of the truth,
        // about 87%; the estimate lies within two standard errors of that.
        let lambda = t.residual_sd_points.powi(2) / 16.0;
        let keep = |n: f64| n / (n + lambda);
        assert!(
            (star.worth_points - 6.0 * keep(60.0)).abs() < 2.0 * star.se_points,
            "worth {} ± {} against {}",
            star.worth_points,
            star.se_points,
            6.0 * keep(60.0)
        );
        assert!(
            star.se_points > 1.0 && star.se_points < 2.0,
            "{}",
            star.se_points
        );
        assert!(star.without_mean < star.with_mean);
        let second = of(2);
        assert_eq!(second.absences, 40);
        assert!(
            (second.worth_points - 3.0 * keep(40.0)).abs() < 2.0 * second.se_points,
            "{} ± {}",
            second.worth_points,
            second.se_points
        );
        // One absence: the prior holds the estimate near zero and the error wide.
        let rare = of(33);
        assert_eq!(rare.absences, 1);
        assert!(rare.worth_points.abs() < 2.0, "{}", rare.worth_points);
        assert!(rare.se_points > 3.0, "{}", rare.se_points);
        assert!(
            t.players
                .windows(2)
                .all(|w| w[0].worth_points >= w[1].worth_points),
            "sorted by worth"
        );
        assert!([PlayerId::new(1), PlayerId::new(2)].contains(&t.players[0].player_id));
        assert_eq!(star.worth_elo, star.worth_points * ELO_PER_POINT);
    }

    #[test]
    fn players_out_together_share_the_gap() {
        // Players 1 and 12 (same team) sit the same games; player 12 is worth nothing.
        let (games, lines) = season(&|p, n| {
            (p == PlayerId::new(1) || p == PlayerId::new(12)) && (20..140).contains(&n)
        });
        let t = table(SeasonYear::of(2025), &games, &lines, PRIOR);
        let of = |p: i32| {
            t.players
                .iter()
                .find(|w| w.player_id == PlayerId::new(p))
                .unwrap()
        };
        // The two are not separable: the ridge splits the gap between them, so the pair
        // adds up to the shrunk worth (2n / (2n + λ) of 6 for n absences each) and
        // neither takes it all.
        let lambda = t.residual_sd_points.powi(2) / 16.0;
        let n = f64::from(of(1).absences);
        let expected = 6.0 * 2.0 * n / (2.0 * n + lambda);
        let sum = of(1).worth_points + of(12).worth_points;
        assert!(
            (sum - expected).abs() < 2.0 * (of(1).se_points + of(12).se_points),
            "{sum} against {expected}"
        );
        assert!((of(1).worth_points - of(12).worth_points).abs() < 1e-6);
        assert!(of(1).worth_points < 4.0);
    }

    #[test]
    fn the_prior_spread_comes_from_the_differences_beyond_their_noise() {
        assert_eq!(estimate_prior_sd(&[(5.0, 1.0); 9]), None, "too few players");
        // Differences of ±5 with a noise variance of 9: τ² = 25 − 9 = 16.
        let diffs: Vec<(f64, f64)> = (0..20)
            .map(|i| (if i % 2 == 0 { 5.0 } else { -5.0 }, 9.0))
            .collect();
        assert!((estimate_prior_sd(&diffs).unwrap() - 4.0).abs() < 1e-9);
        // Nothing beyond the noise: the floor.
        assert_eq!(
            estimate_prior_sd(&[(1.0, 9.0); 20]),
            Some(PRIOR_SD_BOUNDS.0)
        );
        // Huge differences: the cap.
        assert_eq!(
            estimate_prior_sd(&[(40.0, 9.0); 20]),
            Some(PRIOR_SD_BOUNDS.1)
        );
        // The whole table estimates it when the params leave it out, given enough players
        // with absences: here every player sits a block of games.
        let (games, lines) = season(&|p, n| (n / 10) % 6 == (p.as_i32() % 6) as usize);
        let t = table(SeasonYear::of(2025), &games, &lines, WorthParams::default());
        assert!(t.prior_estimated, "{} players", t.players.len());
        assert!(t.prior_sd_points >= PRIOR_SD_BOUNDS.0 && t.prior_sd_points <= PRIOR_SD_BOUNDS.1);
    }

    #[test]
    fn few_games_played_give_no_worth() {
        let (games, mut lines) = season(&|p, n| p == PlayerId::new(1) && (20..60).contains(&n));
        // Player 1 played only four games: no estimate.
        lines.retain(|l| !(l.player_id == PlayerId::new(1) && l.played && l.game_id > 8));
        let t = table(SeasonYear::of(2025), &games, &lines, PRIOR);
        assert!(t.players.iter().all(|w| w.player_id != PlayerId::new(1)));
    }

    #[test]
    fn statuses_map_to_weights_and_spans() {
        assert_eq!(AbsenceStatus::parse("Out"), AbsenceStatus::Out);
        assert_eq!(
            AbsenceStatus::parse("Out For Season"),
            AbsenceStatus::OutForSeason
        );
        assert_eq!(AbsenceStatus::parse("Day-To-Day"), AbsenceStatus::DayToDay);
        assert_eq!(
            AbsenceStatus::parse("Game Time Decision"),
            AbsenceStatus::Questionable
        );
        assert_eq!(AbsenceStatus::parse("probable"), AbsenceStatus::Probable);
        assert_eq!(AbsenceStatus::parse("Out-Indefinitely"), AbsenceStatus::Out);
        assert_eq!(AbsenceStatus::parse("Suspended"), AbsenceStatus::Out);
        assert_eq!(AbsenceStatus::parse("Not With Team"), AbsenceStatus::Out);
        assert_eq!(AbsenceStatus::parse("Not Available"), AbsenceStatus::Out);
        assert_eq!(AbsenceStatus::parse("Available"), AbsenceStatus::Available);
        assert_eq!(AbsenceStatus::parse("  "), AbsenceStatus::Unknown);
        assert_eq!(AbsenceStatus::parse("Rest"), AbsenceStatus::Unknown);
        assert_eq!(AbsenceStatus::Out.weight(), 1.0);
        assert_eq!(AbsenceStatus::Probable.weight(), 0.25);
        assert_eq!(AbsenceStatus::Unknown.weight(), 0.0);
        // An available or unknown status prices nothing.
        let team = TeamId::new(1);
        let line = |status| ReportedAbsence {
            player_id: PlayerId::new(1),
            team_id: team,
            status,
            return_date: None,
            worth_elo: 100.0,
            se_elo: 0.0,
            missed_so_far: Some(0),
        };
        // The error follows the status weight, then the cap, as the penalty does.
        let mut noisy = line(AbsenceStatus::Questionable);
        noisy.se_elo = 100.0;
        let a = absences_from_report(&[noisy], d(1), d(99), None);
        assert_eq!((a[0].elo, a[0].se_elo), (50.0, 50.0));
        let mut capped = line(AbsenceStatus::Out);
        capped.se_elo = 1000.0;
        assert_eq!(
            absences_from_report(&[capped], d(1), d(99), None)[0].se_elo,
            MAX_ABSENCE_ELO
        );
        assert!(
            absences_from_report(&[line(AbsenceStatus::Available)], d(1), d(99), None).is_empty()
        );
        assert!(
            absences_from_report(&[line(AbsenceStatus::Unknown)], d(1), d(99), None).is_empty()
        );
        assert_eq!(
            absences_from_report(&[line(AbsenceStatus::Out)], d(1), d(99), None).len(),
            1
        );
    }

    fn reported(status: AbsenceStatus, return_day: Option<u32>, missed: u32) -> ReportedAbsence {
        reported_with(status, return_day, Some(missed))
    }

    fn reported_with(
        status: AbsenceStatus,
        return_day: Option<u32>,
        missed: Option<u32>,
    ) -> ReportedAbsence {
        ReportedAbsence {
            player_id: PlayerId::new(1),
            team_id: TeamId::new(1),
            status,
            return_date: return_day.map(d),
            worth_elo: 100.0,
            se_elo: 0.0,
            missed_so_far: missed,
        }
    }

    #[test]
    fn the_span_follows_the_return_date_the_curve_or_the_status() {
        use crate::domain::spell::{Run, Survival};
        // Runs of 1, 1, 2, 3 and 5 games: a run two games old has two games to go, a new
        // one has 2.4, rounded to two.
        let runs: Vec<Run> = [1u32, 1, 2, 3, 5]
            .iter()
            .cycle()
            .take(60)
            .map(|m| Run {
                missed: *m,
                censored: false,
            })
            .collect();
        let survival = Survival::fit(&runs).unwrap();
        let mut upcoming = HashMap::new();
        // The team plays every second day from day 11.
        upcoming.insert(
            TeamId::new(1),
            (0..10).map(|i| d(11 + 2 * i)).collect::<Vec<_>>(),
        );
        let rule = SpanRule {
            survival: Some(&survival),
            statuses: None,
            upcoming: &upcoming,
        };
        let report = d(10);
        let span = |r: &ReportedAbsence, rule: Option<&SpanRule>| {
            span_for(r, report, d(40), rule).map(|s| (s.to, s.source))
        };
        // A return date that is the next game is the feed's placeholder: two games to go.
        assert_eq!(
            span(&reported(AbsenceStatus::Out, Some(11), 2), Some(&rule)),
            Some((d(13), SpanSource::Expected))
        );
        // Without a date, the same.
        assert_eq!(
            span(&reported(AbsenceStatus::Out, None, 2), Some(&rule)),
            Some((d(13), SpanSource::Expected))
        );
        // A date beyond the next game is an estimate and stands.
        assert_eq!(
            span(&reported(AbsenceStatus::Out, Some(20), 2), Some(&rule)),
            Some((d(19), SpanSource::ReturnDate))
        );
        // Without the game logs, a return date stands as it did, and no date means the
        // status default: fourteen days.
        assert_eq!(
            span(&reported(AbsenceStatus::Out, Some(11), 2), None),
            Some((d(10), SpanSource::ReturnDate))
        );
        assert_eq!(
            span(&reported(AbsenceStatus::Out, None, 2), None),
            Some((d(23), SpanSource::Status))
        );
        // Questionable for tonight stays a two-day matter; in a run of three it is a spell.
        assert_eq!(
            span(
                &reported(AbsenceStatus::Questionable, Some(11), 0),
                Some(&rule)
            ),
            Some((d(11), SpanSource::Status))
        );
        assert_eq!(
            span(&reported(AbsenceStatus::DayToDay, Some(11), 5), Some(&rule)),
            Some((d(11), SpanSource::Expected)),
            "past the longest run, one game"
        );
        // Absent from the lines: priced as out.
        assert_eq!(
            span(&reported(AbsenceStatus::Absent, None, 3), Some(&rule)),
            Some((d(13), SpanSource::Expected))
        );
        // Out for the season, a date in the past, nothing to price.
        assert_eq!(
            span(
                &reported(AbsenceStatus::OutForSeason, Some(11), 0),
                Some(&rule)
            ),
            Some((d(40), SpanSource::Season))
        );
        // A date of today is the placeholder too for a player in a spell; for a line about
        // tonight it prices nothing, as before, and so does any date of today without the
        // game logs.
        assert_eq!(
            span(&reported(AbsenceStatus::Out, Some(10), 2), Some(&rule)),
            Some((d(13), SpanSource::Expected))
        );
        assert_eq!(
            span(
                &reported(AbsenceStatus::Questionable, Some(10), 0),
                Some(&rule)
            ),
            None
        );
        assert_eq!(span(&reported(AbsenceStatus::Out, Some(10), 2), None), None);
        assert_eq!(
            span(&reported(AbsenceStatus::Available, None, 9), Some(&rule)),
            None
        );
        // A team without box scores: the games missed are unknown, the status default
        // stands and a placeholder date of today prices nothing.
        assert_eq!(
            span(
                &reported_with(AbsenceStatus::Out, Some(11), None),
                Some(&rule)
            ),
            Some((d(23), SpanSource::Status))
        );
        assert_eq!(
            span(
                &reported_with(AbsenceStatus::Out, Some(10), None),
                Some(&rule)
            ),
            None
        );
        // More games to go than the schedule holds: the season's end.
        let mut short = HashMap::new();
        short.insert(TeamId::new(1), vec![d(11)]);
        let rule = SpanRule {
            survival: Some(&survival),
            statuses: None,
            upcoming: &short,
        };
        assert_eq!(
            span(&reported(AbsenceStatus::Out, None, 0), Some(&rule)),
            Some((d(40), SpanSource::Expected))
        );
    }

    #[test]
    fn the_status_curve_sets_the_weight_and_the_span() {
        use crate::domain::spell::{Run, Survival};
        use crate::domain::status::{StatusSample, StatusTable};
        // Day-to-day lines: not yet out, three in five miss the next two games; already
        // in a run, every line misses three more games.
        let mut samples = Vec::new();
        for i in 0..50 {
            samples.push(StatusSample {
                status: AbsenceStatus::DayToDay,
                in_run: false,
                missed: if i < 30 {
                    vec![true, true, false]
                } else {
                    vec![false, false, false]
                },
            });
            samples.push(StatusSample {
                status: AbsenceStatus::DayToDay,
                in_run: true,
                missed: vec![true, true, true, false],
            });
        }
        let statuses = StatusTable::fit(&samples);
        let runs: Vec<Run> = std::iter::repeat_n(
            Run {
                missed: 5,
                censored: false,
            },
            60,
        )
        .collect();
        let survival = Survival::fit(&runs).unwrap();
        let mut upcoming = HashMap::new();
        upcoming.insert(
            TeamId::new(1),
            (0..10).map(|i| d(11 + 2 * i)).collect::<Vec<_>>(),
        );
        let rule = SpanRule {
            survival: Some(&survival),
            statuses: Some(&statuses),
            upcoming: &upcoming,
        };
        let report = d(10);
        let span = |r: &ReportedAbsence| span_for(r, report, d(40), Some(&rule)).unwrap();
        // Not yet out, the placeholder date: the measured weight over the measured span,
        // 1.2 games missed at 0.6 is two games.
        let fresh = span(&reported(AbsenceStatus::DayToDay, Some(11), 0));
        assert_eq!(
            (fresh.to, fresh.source, fresh.weight_source),
            (d(13), SpanSource::Measured, WeightSource::Measured)
        );
        assert!((fresh.weight - 0.6).abs() < 1e-12);
        // A date of today is the placeholder too once a curve backs the status.
        assert_eq!(
            span(&reported(AbsenceStatus::DayToDay, Some(10), 0)).to,
            d(13)
        );
        // Already in a run of one: the in-run curve, every line misses three.
        let in_run = span(&reported(AbsenceStatus::DayToDay, None, 1));
        assert_eq!((in_run.to, in_run.source), (d(15), SpanSource::Measured));
        assert_eq!(in_run.weight, 1.0);
        // In a spell of three or more the survival curve sets the span; the weight is
        // still the in-run curve's.
        let spell = span(&reported(AbsenceStatus::DayToDay, Some(11), 3));
        assert_eq!(
            (spell.source, spell.weight_source, spell.weight),
            (SpanSource::Expected, WeightSource::Measured, 1.0)
        );
        // A date beyond the next game stands, at the status's default weight.
        let dated = span(&reported(AbsenceStatus::DayToDay, Some(20), 0));
        assert_eq!(
            (dated.to, dated.source, dated.weight, dated.weight_source),
            (d(19), SpanSource::ReturnDate, 0.5, WeightSource::Status)
        );
        // A status measured as never missing the next game prices nothing.
        for _ in 0..50 {
            samples.push(StatusSample {
                status: AbsenceStatus::Probable,
                in_run: false,
                missed: vec![false, false],
            });
        }
        let with_probable = StatusTable::fit(&samples);
        let never = SpanRule {
            survival: Some(&survival),
            statuses: Some(&with_probable),
            upcoming: &upcoming,
        };
        assert_eq!(
            span_for(
                &reported(AbsenceStatus::Probable, Some(11), 0),
                report,
                d(40),
                Some(&never)
            ),
            None
        );
        // A status without a curve keeps its defaults.
        let q = span(&reported(AbsenceStatus::Questionable, Some(11), 0));
        assert_eq!(
            (q.to, q.source, q.weight, q.weight_source),
            (d(11), SpanSource::Status, 0.5, WeightSource::Status)
        );
        let out = span(&reported(AbsenceStatus::Out, None, 2));
        assert_eq!((out.source, out.weight), (SpanSource::Expected, 1.0));
        assert_eq!(
            span(&reported(AbsenceStatus::OutForSeason, None, 0)).weight,
            1.0
        );
        // A team without box scores: the state is unknown, the defaults stand.
        let unknown = span(&reported_with(AbsenceStatus::DayToDay, None, None));
        assert_eq!(
            (unknown.source, unknown.weight_source),
            (SpanSource::Status, WeightSource::Status)
        );
        // The penalty takes the measured weight.
        let a = absences_from_report(
            &[reported(AbsenceStatus::DayToDay, Some(11), 0)],
            report,
            d(40),
            Some(&rule),
        );
        assert_eq!((a[0].elo, a[0].to), (60.0, d(13)));
        // Status curves without a survival curve: out keeps its default span.
        let rule = SpanRule {
            survival: None,
            statuses: Some(&statuses),
            upcoming: &upcoming,
        };
        let out = span_for(
            &reported(AbsenceStatus::Out, Some(11), 2),
            report,
            d(40),
            Some(&rule),
        )
        .unwrap();
        assert_eq!((out.to, out.source), (d(23), SpanSource::Status));
        // A rule with neither curve is no rule: the placeholder date stands as it did.
        let bare = SpanRule {
            survival: None,
            statuses: None,
            upcoming: &upcoming,
        };
        let out = span_for(
            &reported(AbsenceStatus::Out, Some(11), 2),
            report,
            d(40),
            Some(&bare),
        )
        .unwrap();
        assert_eq!((out.to, out.source), (d(10), SpanSource::ReturnDate));
        assert_eq!(
            span_for(
                &reported(AbsenceStatus::Out, Some(10), 2),
                report,
                d(40),
                Some(&bare)
            ),
            None
        );
    }

    #[test]
    fn the_effect_sums_the_penalties_and_the_win_probability_over_the_games_covered() {
        use crate::domain::models::{EloSource, GameType, ResultSource};
        let game = |day: u32, home: i32, away: i32| Game {
            id: i64::from(day),
            season: SeasonYear::of(2026),
            game_date: d(day),
            seq: 0,
            home_team_id: TeamId::new(home),
            away_team_id: TeamId::new(away),
            neutral: false,
            game_type: GameType::Regular,
            home_score: None,
            away_score: None,
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
        };
        // Team 1 plays on days 10 (home), 12 (away) and 14 (home); day 8 is played.
        let mut played = game(8, 1, 2);
        played.home_score = Some(100);
        played.away_score = Some(90);
        let games = vec![played, game(10, 1, 2), game(12, 2, 1), game(14, 1, 3)];
        let mut ratings = HashMap::new();
        ratings.insert(TeamId::new(1), 1600.0);
        ratings.insert(TeamId::new(2), 1500.0);
        let absence = |to: u32, elo: f64| Absence {
            team_id: TeamId::new(1),
            from: d(9),
            to: d(to),
            elo,
            player_id: Some(PlayerId::new(1)),
            se_elo: 0.0,
        };
        let listed = vec![absence(12, 100.0), absence(10, 50.0)];
        let e = absence_effect(&listed, &games, &ratings, TeamId::new(1), d(10), 60.0);
        assert_eq!((e.games, e.games_covered), (3, 2));
        assert!(
            (e.elo_games - 250.0).abs() < 1e-9,
            "150 on day 10, 100 on day 12"
        );
        assert!((e.mean_penalty - 250.0 / 3.0).abs() < 1e-9);
        let p = |elo: f64, home: bool, other: f64| {
            elo::game_probability(elo, other, home, false, 60.0, 1.0)
        };
        let expected = (p(1600.0, true, 1500.0) - p(1450.0, true, 1500.0))
            + (p(1600.0, false, 1500.0) - p(1500.0, false, 1500.0));
        assert!((e.wins_lost - expected).abs() < 1e-12);
        // Held to day 14: both absences cover every game; team 3 is read at the mean.
        let kept = held(&listed, d(10), d(14));
        assert_eq!(kept.len(), 2);
        assert!(kept.iter().all(|a| a.to == d(14)));
        let h = absence_effect(&kept, &games, &ratings, TeamId::new(1), d(10), 60.0);
        assert_eq!((h.games, h.games_covered), (3, 3));
        assert!((h.elo_games - 450.0).abs() < 1e-9);
        assert!(h.wins_lost > e.wins_lost);
        // An absence that ended before today is not held, and a team without games has
        // no effect.
        assert!(held(&[absence(9, 100.0)], d(10), d(14)).is_empty());
        let none = absence_effect(&listed, &games, &ratings, TeamId::new(9), d(10), 60.0);
        assert_eq!(none.games, 0);
        assert_eq!(none.mean_penalty, 0.0);
    }

    #[test]
    fn tonight_prices_the_next_nights_unplayed_games() {
        use crate::domain::models::{EloSource, GameType, ResultSource};
        let game = |id: i64, day: u32, home: i32, away: i32, game_type: GameType| Game {
            id,
            season: SeasonYear::of(2026),
            game_date: d(day),
            seq: 0,
            home_team_id: TeamId::new(home),
            away_team_id: TeamId::new(away),
            neutral: false,
            game_type,
            home_score: None,
            away_score: None,
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
        };
        let played = |mut g: Game| {
            g.home_score = Some(100);
            g.away_score = Some(90);
            g
        };
        let games = vec![
            // A played game on the night, an unplayed one before the date (a postponed
            // game is not tonight), a playoff game earlier, and the night's two games.
            played(game(1, 10, 1, 2, GameType::Regular)),
            game(2, 8, 3, 4, GameType::Regular),
            game(3, 9, 1, 3, GameType::Playoff),
            game(4, 10, 2, 3, GameType::Regular),
            game(5, 10, 4, 1, GameType::Regular),
            game(6, 14, 1, 3, GameType::Regular),
        ];
        let mut ratings = HashMap::new();
        ratings.insert(TeamId::new(1), 1600.0);
        ratings.insert(TeamId::new(2), 1500.0);
        ratings.insert(TeamId::new(4), 1550.0);
        let absences = vec![Absence {
            team_id: TeamId::new(1),
            from: d(9),
            to: d(12),
            elo: 150.0,
            player_id: Some(PlayerId::new(1)),
            se_elo: 0.0,
        }];
        let night = tonight(&games, d(10), &ratings, &absences, 60.0).unwrap();
        assert_eq!((night.date, night.played, night.games.len()), (d(10), 1, 2));
        let p = |h: f64, a: f64| elo::game_probability(h, a, true, false, 60.0, 1.0);
        // Game 5: team 4 hosts team 1, who plays 150 down.
        let g = night.games.iter().find(|g| g.game_id == 5).unwrap();
        assert_eq!(
            (
                g.away_rating,
                g.away_penalty,
                g.away_tonight,
                g.home_penalty
            ),
            (1600.0, 150.0, 1450.0, 0.0)
        );
        assert!((g.p_home_full - p(1550.0, 1600.0)).abs() < 1e-12);
        assert!((g.p_home_tonight - p(1550.0, 1450.0)).abs() < 1e-12);
        // Team 3 has no rating: the league mean stands in, and the game says so.
        let g = night.games.iter().find(|g| g.game_id == 4).unwrap();
        assert_eq!(
            (g.away_rating, g.away_rated, g.home_rated),
            (elo::SEASON_MEAN, false, true)
        );
        // A later date finds the later night; past the last game there is none.
        let later = tonight(&games, d(11), &ratings, &absences, 60.0).unwrap();
        assert_eq!((later.date, later.played, later.games.len()), (d(14), 0, 1));
        assert!(tonight(&games, d(15), &ratings, &absences, 60.0).is_none());
    }

    #[test]
    fn a_description_that_ends_the_season_outranks_the_status() {
        assert_eq!(
            AbsenceStatus::parse_line("Out", Some("Suffered a season-ending ACL tear")),
            AbsenceStatus::OutForSeason
        );
        assert_eq!(
            AbsenceStatus::parse_line("Day-To-Day", Some("Will miss the rest of the season")),
            AbsenceStatus::OutForSeason
        );
        assert_eq!(
            AbsenceStatus::parse_line("Available", Some("Back for the season opener")),
            AbsenceStatus::Available
        );
        assert_eq!(
            AbsenceStatus::parse_line("Out", Some("Knee; out")),
            AbsenceStatus::Out
        );
        assert_eq!(AbsenceStatus::parse_line("Out", None), AbsenceStatus::Out);
        // A game of the season, or an injury avoided, is not the season.
        assert_eq!(
            AbsenceStatus::parse_line("Questionable", Some("Questionable for the season opener")),
            AbsenceStatus::Questionable
        );
        assert_eq!(
            AbsenceStatus::parse_line("Out", Some("Out for the season debut with a sore knee")),
            AbsenceStatus::Out
        );
        assert_eq!(
            AbsenceStatus::parse_line("Out", Some("Avoided a season-ending injury, per the team")),
            AbsenceStatus::Out
        );
        assert_eq!(
            AbsenceStatus::parse_line("Out", Some("Ruled out for the season (Achilles)")),
            AbsenceStatus::OutForSeason
        );
    }

    #[test]
    fn a_report_becomes_penalties_bounded_by_the_return_date_and_the_season() {
        let team = TeamId::new(1);
        let report = d(10);
        let reported = vec![
            ReportedAbsence {
                player_id: PlayerId::new(1),
                team_id: team,
                status: AbsenceStatus::Out,
                return_date: Some(d(15)),
                worth_elo: 120.0,
                se_elo: 0.0,
                missed_so_far: Some(0),
            },
            ReportedAbsence {
                player_id: PlayerId::new(2),
                team_id: team,
                status: AbsenceStatus::Questionable,
                return_date: None,
                worth_elo: 60.0,
                se_elo: 0.0,
                missed_so_far: Some(0),
            },
            ReportedAbsence {
                player_id: PlayerId::new(3),
                team_id: team,
                status: AbsenceStatus::OutForSeason,
                return_date: None,
                worth_elo: 900.0,
                se_elo: 0.0,
                missed_so_far: Some(0),
            },
            // Back already, worthless, or a return date in the past: no penalty.
            ReportedAbsence {
                player_id: PlayerId::new(4),
                team_id: team,
                status: AbsenceStatus::Out,
                return_date: Some(d(10)),
                worth_elo: 50.0,
                se_elo: 0.0,
                missed_so_far: Some(0),
            },
            ReportedAbsence {
                player_id: PlayerId::new(5),
                team_id: team,
                status: AbsenceStatus::Out,
                return_date: None,
                worth_elo: -20.0,
                se_elo: 0.0,
                missed_so_far: Some(0),
            },
        ];
        let out = absences_from_report(&reported, report, d(40), None);
        assert_eq!(out.len(), 3);
        assert_eq!((out[0].from, out[0].to, out[0].elo), (d(10), d(14), 120.0));
        assert_eq!((out[1].from, out[1].to, out[1].elo), (d(10), d(11), 30.0));
        assert_eq!(
            (out[2].to, out[2].elo),
            (d(40), MAX_ABSENCE_ELO),
            "capped, to the season's end"
        );
        assert_eq!(penalty(&out, team, d(10)), 120.0 + 30.0 + MAX_ABSENCE_ELO);
        assert_eq!(penalty(&out, team, d(14)), 120.0 + MAX_ABSENCE_ELO);
        assert_eq!(penalty(&out, team, d(15)), MAX_ABSENCE_ELO);
        assert_eq!(penalty(&out, TeamId::new(2), d(10)), 0.0);
    }

    #[test]
    fn the_expected_penalty_is_the_mean_of_the_held_draw() {
        // Reference values from numerical integration of clamp(N(elo, se), 0, 400).
        for (elo, se, want) in [
            (0.0, 60.0, 23.9),
            (20.0, 70.0, 39.1),
            (60.0, 60.0, 65.0),
            (100.0, 100.0, 108.3),
            (150.0, 60.0, 150.1),
            (380.0, 60.0, 364.7),
        ] {
            let got = expected_penalty(elo, se);
            assert!((got - want).abs() < 0.1, "{elo} ± {se}: {got} vs {want}");
        }
        assert_eq!(expected_penalty(150.0, 0.0), 150.0);
        assert_eq!(expected_penalty(-20.0, 0.0), 0.0);
        let a = Absence {
            team_id: TeamId::new(1),
            from: d(1),
            to: d(5),
            elo: 0.0,
            player_id: None,
            se_elo: 60.0,
        };
        assert!((penalty(&[a], TeamId::new(1), d(3)) - 23.9).abs() < 0.1);
    }
}
