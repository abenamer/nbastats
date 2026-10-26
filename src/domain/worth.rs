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

    /// The share of the player's worth the status removes.
    pub fn weight(self) -> f64 {
        match self {
            Self::Out | Self::OutForSeason => 1.0,
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
            Self::Out => 14,
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
}

/// The penalties of a report dated `report_date`. A line lasts until the day before the
/// return date when the source gives one, else for the status's default span; never past
/// `season_end`. A worth at or below zero gives no penalty: the model does not reward an
/// absence. A status that removes nothing (available, unknown) gives none either.
pub fn absences_from_report(
    reported: &[ReportedAbsence],
    report_date: NaiveDate,
    season_end: NaiveDate,
) -> Vec<Absence> {
    reported
        .iter()
        .filter(|r| r.worth_elo > 0.0)
        .filter_map(|r| {
            let to = match r.return_date {
                Some(d) if d > report_date => d - Duration::days(1),
                Some(_) => return None,
                None => report_date + Duration::days(r.status.default_days() - 1),
            };
            let to = to.min(season_end);
            if to < report_date {
                return None;
            }
            let elo = (r.worth_elo * r.status.weight()).min(MAX_ABSENCE_ELO);
            let se_elo = (r.se_elo.max(0.0) * r.status.weight()).min(MAX_ABSENCE_ELO);
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

/// The penalty of `team` on `date`: the sum of its absences that cover the date. An
/// absence with a standard error counts its expected drawn penalty, see
/// [`expected_penalty`].
pub fn penalty(absences: &[Absence], team: TeamId, date: NaiveDate) -> f64 {
    absences
        .iter()
        .filter(|a| a.team_id == team && a.covers(date))
        .map(|a| expected_penalty(a.elo, a.se_elo))
        .sum()
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
        };
        // The error follows the status weight, then the cap, as the penalty does.
        let mut noisy = line(AbsenceStatus::Questionable);
        noisy.se_elo = 100.0;
        let a = absences_from_report(&[noisy], d(1), d(99));
        assert_eq!((a[0].elo, a[0].se_elo), (50.0, 50.0));
        let mut capped = line(AbsenceStatus::Out);
        capped.se_elo = 1000.0;
        assert_eq!(
            absences_from_report(&[capped], d(1), d(99))[0].se_elo,
            MAX_ABSENCE_ELO
        );
        assert!(absences_from_report(&[line(AbsenceStatus::Available)], d(1), d(99)).is_empty());
        assert!(absences_from_report(&[line(AbsenceStatus::Unknown)], d(1), d(99)).is_empty());
        assert_eq!(
            absences_from_report(&[line(AbsenceStatus::Out)], d(1), d(99)).len(),
            1
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
            },
            ReportedAbsence {
                player_id: PlayerId::new(2),
                team_id: team,
                status: AbsenceStatus::Questionable,
                return_date: None,
                worth_elo: 60.0,
                se_elo: 0.0,
            },
            ReportedAbsence {
                player_id: PlayerId::new(3),
                team_id: team,
                status: AbsenceStatus::OutForSeason,
                return_date: None,
                worth_elo: 900.0,
                se_elo: 0.0,
            },
            // Back already, worthless, or a return date in the past: no penalty.
            ReportedAbsence {
                player_id: PlayerId::new(4),
                team_id: team,
                status: AbsenceStatus::Out,
                return_date: Some(d(10)),
                worth_elo: 50.0,
                se_elo: 0.0,
            },
            ReportedAbsence {
                player_id: PlayerId::new(5),
                team_id: team,
                status: AbsenceStatus::Out,
                return_date: None,
                worth_elo: -20.0,
                se_elo: 0.0,
            },
        ];
        let out = absences_from_report(&reported, report, d(40));
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
