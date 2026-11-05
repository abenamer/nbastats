//! YAXEL, the emergence term: a team's rating moved by what its players' recent box scores
//! say beyond the worth each was priced at coming into the season. Named for the rookie
//! whose first weeks the market priced and the rating, which learns from results at K = 20,
//! did not.
//!
//! A player's surprise is his box worth over the window of games before tonight, shrunk
//! toward the worth he entered the season with by his minutes, less that worth. A team's
//! shift is the damped sum of the surprises of the players on its floor. The forecast of a
//! game is the Elo forecast with both teams' shifts added. Everything here is pure: the
//! application builds the samples from the stored games, lines and priors, and this module
//! scores the rule against the plain Elo forecast over a grid of its three parameters, with
//! two folds of seasons so the parameters are never chosen on the seasons they are scored on.

use std::collections::{HashMap, HashSet};

use chrono::{Duration, NaiveDate};
use serde::{Deserialize, Serialize};

use crate::domain::bpm::BoxLine;
use crate::domain::elo;
use crate::domain::full_strength::{self, MissingElo};
use crate::domain::ids::{PlayerId, SeasonYear, TeamId};
use crate::domain::models::Game;
use crate::domain::spell::line_played;

pub const MODEL_VERSION: &str = "yaxel-1.0";

/// The three parameters of the rule.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Params {
    /// The window of box scores before the game, in days.
    pub window_days: u16,
    /// The minutes at which a player's recent box worth and his entering worth weigh the
    /// same; fewer minutes lean on the entering worth.
    pub prior_minutes: f64,
    /// The share of the summed surprises that moves the team.
    pub damping: f64,
}

/// The grid the backtest searches.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Grid {
    pub window_days: Vec<u16>,
    pub prior_minutes: Vec<f64>,
    pub dampings: Vec<f64>,
}

impl Default for Grid {
    fn default() -> Self {
        Self {
            window_days: vec![7, 14, 28],
            prior_minutes: vec![100.0, 250.0, 500.0],
            dampings: vec![0.25, 0.5, 0.75, 1.0],
        }
    }
}

impl Grid {
    /// Every combination, window first.
    pub fn params(&self) -> Vec<Params> {
        let mut out = Vec::new();
        for &window_days in &self.window_days {
            for &prior_minutes in &self.prior_minutes {
                for &damping in &self.dampings {
                    out.push(Params {
                        window_days,
                        prior_minutes,
                        damping,
                    });
                }
            }
        }
        out
    }
}

/// A player on a team's floor for a game: the worth he was priced at entering the season,
/// his box worth and minutes over the window before the game, when he has any, whether he
/// is new to the league, and his minutes in the game that put him on the floor.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Floor {
    pub prior_elo: f64,
    pub recent: Option<(f64, f64)>,
    /// A new player: no line in any stored season before this one, so the rating has no
    /// record of him and his entering worth is a draft bracket or the replacement level.
    #[serde(default)]
    pub new: bool,
    /// His minutes in the team's previous game.
    #[serde(default)]
    pub last_minutes: f64,
}

/// Whose surprises the term sums: every player on the floor, or the new players alone.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Population {
    All,
    New,
}

impl Population {
    fn includes(self, f: &Floor) -> bool {
        match self {
            Population::All => true,
            Population::New => f.new,
        }
    }
}

/// Whether a new player on the floor played at least `minutes` in the team's previous game.
/// A floor without a new player never qualifies, whatever the minutes asked.
pub fn has_new_player(floor: &[Floor], minutes: f64) -> bool {
    floor.iter().any(|f| f.new && f.last_minutes >= minutes)
}

/// What the live forecast would price a side at, with the box scores standing in for the
/// injury report: the full-strength pre-game rating (the stored rating with the absences of
/// the games played taken out of the expectation, `domain::full_strength`), and the Elo
/// the side is missing tonight, read two ways: from its previous game, the players of its
/// roster who sat it, which is what the backtest can know before tip-off; and from
/// tonight's box score, the players who sat tonight, which the report tells the live page
/// before the game for the players it lists out. The absence-penalized rating is the
/// full-strength rating less either.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Strength {
    pub full_pre: f64,
    pub missing_last: f64,
    pub missing_tonight: f64,
}

impl Strength {
    /// A side with nobody missing: the stored rating is the full one.
    pub fn plain(elo: f64) -> Self {
        Self {
            full_pre: elo,
            missing_last: 0.0,
            missing_tonight: 0.0,
        }
    }

    /// The rating less the previous game's absentees.
    pub fn less_last(&self) -> f64 {
        self.full_pre - self.missing_last
    }

    /// The rating less tonight's absentees.
    pub fn less_tonight(&self) -> f64 {
        self.full_pre - self.missing_tonight
    }
}

/// One regular-season game as the backtest sees it before tip-off: the stored pre-game
/// ratings, each side's strength as the live forecast would read it, the result, each
/// team's game number in the season, and per window of the grid (in the grid's order) the
/// two floors.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GameSample {
    pub game_date: NaiveDate,
    pub home_game_no: u32,
    pub away_game_no: u32,
    pub home_elo_pre: f64,
    pub away_elo_pre: f64,
    pub home_strength: Strength,
    pub away_strength: Strength,
    pub neutral: bool,
    pub home_won: bool,
    pub floors: Vec<(Vec<Floor>, Vec<Floor>)>,
}

/// The Elo each side was missing in each played game, by `(game_id, team)`, read from the
/// box scores in place of the injury report (`full_strength::game_absences` reads the
/// reports, which the stored seasons before the feed do not have). A player is on a team's
/// roster over his stints with it ([`stints`]): from his first game for the team, or from
/// the season's start when he ended the season before with it, to the day before his first
/// game for another team, or the season's end. So a player out from the first game who
/// returns is missing until he does, a player signed in season is not, a player traded
/// away leaves the roster when he plays for his new team, and a player who never plays for
/// the team is not on it. `tonight` reads each game's own box score: the roster's players
/// with no line played, priced by `full_strength::missing_elo`; nothing for a team whose
/// box score is not known whole (fewer than [`MIN_PLAYED_LINES`] played lines). `last`
/// reads, for each game, the roster's players who were on it at the team's previous game
/// and sat it: the lineup that game's floor tells before tip-off, on a roster read from the
/// season's lines as a report would list it; nothing without a previous game known whole.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct BoxAbsences {
    pub tonight: MissingElo,
    pub last: MissingElo,
}

/// A team's box score tells who sat only when at least this many of its players played:
/// five are on the floor at any time, so fewer is a box score loaded in part.
pub const MIN_PLAYED_LINES: usize = 5;

/// One player's stint with a team: the span of dates he is on its roster.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stint {
    pub team: TeamId,
    pub from: NaiveDate,
    pub to: NaiveDate,
}

/// A player's stints from his played games of a season, `(date, team)` in any order, given
/// the team he ended the season before with when known. A stint starts at his first game
/// for the team, or at the season's start for his first stint when the team is the one he
/// ended the season before with; it runs to the day before his first game for another
/// team, or to the season's end. The days between two stints belong to the team he left.
pub fn stints(played: &[(NaiveDate, TeamId)], carried: Option<TeamId>) -> Vec<Stint> {
    let mut played = played.to_vec();
    played.sort_unstable();
    let mut out = Vec::new();
    let mut i = 0;
    while i < played.len() {
        let team = played[i].1;
        let mut j = i;
        while j < played.len() && played[j].1 == team {
            j += 1;
        }
        out.push(Stint {
            team,
            from: if i == 0 && carried == Some(team) {
                NaiveDate::MIN
            } else {
                played[i].0
            },
            to: if j == played.len() {
                NaiveDate::MAX
            } else {
                played[j].0 - Duration::days(1)
            },
        });
        i = j;
    }
    out
}

/// The team each player ended a season with: that of his latest played line among the
/// season's played games. The roster a team carries into the next season, as far as the
/// box scores tell it.
pub fn last_teams(games: &[Game], lines: &[BoxLine]) -> HashMap<PlayerId, TeamId> {
    let order: HashMap<i64, _> = games
        .iter()
        .filter(|g| g.is_played())
        .map(|g| (g.id, (g.game_date, g.seq, g.id)))
        .collect();
    let mut latest: HashMap<PlayerId, (_, TeamId)> = HashMap::new();
    for l in lines.iter().filter(|l| line_played(l)) {
        let Some(&key) = order.get(&l.game_id) else {
            continue;
        };
        let e = latest.entry(l.player_id).or_insert((key, l.team_id));
        if key >= e.0 {
            *e = (key, l.team_id);
        }
    }
    latest.into_iter().map(|(p, (_, t))| (p, t)).collect()
}

/// Read the absences of the played games from their box scores, each player at the
/// expected penalty of his worth `(elo, se)`; `carried` is the team each player ended the
/// season before with ([`last_teams`]), which puts him on its roster from the season's
/// start.
pub fn box_absences(
    games: &[Game],
    lines: &[BoxLine],
    worth_of: &HashMap<PlayerId, (f64, f64)>,
    carried: &HashMap<PlayerId, TeamId>,
) -> BoxAbsences {
    let mut ordered: Vec<&Game> = games.iter().filter(|g| g.is_played()).collect();
    ordered.sort_by_key(|g| (g.game_date, g.seq, g.id));
    let date_of: HashMap<i64, NaiveDate> = ordered.iter().map(|g| (g.id, g.game_date)).collect();
    // Who played for each team in each game, and each player's played games by date.
    let mut played_in: HashMap<(i64, TeamId), HashSet<PlayerId>> = HashMap::new();
    let mut appearances: HashMap<PlayerId, Vec<(NaiveDate, TeamId)>> = HashMap::new();
    for l in lines.iter().filter(|l| line_played(l)) {
        let Some(&date) = date_of.get(&l.game_id) else {
            continue;
        };
        played_in
            .entry((l.game_id, l.team_id))
            .or_default()
            .insert(l.player_id);
        appearances
            .entry(l.player_id)
            .or_default()
            .push((date, l.team_id));
    }
    // A box score known whole, else nothing is read from it.
    let played_in = |g: &Game, team: TeamId| -> Option<&HashSet<PlayerId>> {
        played_in
            .get(&(g.id, team))
            .filter(|p| p.len() >= MIN_PLAYED_LINES)
    };
    // Each team's roster as its players' stints.
    let mut rosters: HashMap<TeamId, Vec<(PlayerId, Stint)>> = HashMap::new();
    for (player, played) in &appearances {
        for stint in stints(played, carried.get(player).copied()) {
            rosters
                .entry(stint.team)
                .or_default()
                .push((*player, stint));
        }
    }
    let roster = |team: TeamId, date: NaiveDate| {
        rosters
            .get(&team)
            .into_iter()
            .flatten()
            .filter(move |(_, s)| s.from <= date && date <= s.to)
            .map(|(p, _)| p)
    };
    // Each team's previous game: its date and who played it, when known whole.
    let mut previous: HashMap<TeamId, (NaiveDate, Option<&HashSet<PlayerId>>)> = HashMap::new();
    let mut out = BoxAbsences::default();
    for g in &ordered {
        for team in [g.home_team_id, g.away_team_id] {
            let played = played_in(g, team);
            if let Some(played) = played {
                let tonight = full_strength::missing_elo(
                    roster(team, g.game_date).filter(|p| !played.contains(p)),
                    worth_of,
                );
                if tonight > 0.0 {
                    out.tonight.insert((g.id, team), tonight);
                }
            }
            if let Some((date, Some(played_before))) = previous.get(&team) {
                let on_roster_then: HashSet<&PlayerId> = roster(team, *date).collect();
                let last = full_strength::missing_elo(
                    roster(team, g.game_date)
                        .filter(|p| on_roster_then.contains(p) && !played_before.contains(p)),
                    worth_of,
                );
                if last > 0.0 {
                    out.last.insert((g.id, team), last);
                }
            }
            previous.insert(team, (g.game_date, played));
        }
    }
    out
}

/// Each played game's two sides as the live forecast would price them (`Strength`), by
/// game id. The full-strength pre-game rating is the stored one moved by the gap between
/// the replay with the games' absences (`absences.tonight`) and the replay without them,
/// the gap held within `full_strength::MAX_SHIFT_ELO` as a forecast holds it; a gap that is
/// not finite (a stored rating that is not) moves nothing.
pub fn strengths(games: &[Game], absences: &BoxAbsences) -> HashMap<i64, (Strength, Strength)> {
    let full = full_strength::pregame_ratings(games, &absences.tonight);
    let plain = full_strength::pregame_ratings(games, &MissingElo::new());
    let cap = full_strength::MAX_SHIFT_ELO;
    let shift = |full: f64, plain: f64| {
        let gap = full - plain;
        if gap.is_finite() {
            gap.clamp(-cap, cap)
        } else {
            0.0
        }
    };
    games
        .iter()
        .filter(|g| g.is_played())
        .filter_map(|g| {
            let (full_home, full_away) = full.get(&g.id)?;
            let (plain_home, plain_away) = plain.get(&g.id)?;
            let side = |stored: f64, full: f64, plain: f64, team: TeamId| Strength {
                full_pre: stored + shift(full, plain),
                missing_last: absences.last.get(&(g.id, team)).copied().unwrap_or(0.0),
                missing_tonight: absences.tonight.get(&(g.id, team)).copied().unwrap_or(0.0),
            };
            Some((
                g.id,
                (
                    side(g.home_elo_pre, *full_home, *plain_home, g.home_team_id),
                    side(g.away_elo_pre, *full_away, *plain_away, g.away_team_id),
                ),
            ))
        })
        .collect()
}

/// A season's samples with the home bonus its forecasts use, and whether its new players
/// could be told: the first stored season of box scores has no earlier line for anyone, so
/// every player drafted within five years, or without a draft year, would look new, and no
/// game of the season counts as one with a new player on a floor; the season is left out of
/// the new-player cut and of the run on the new players alone.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SeasonSample {
    pub season: SeasonYear,
    pub home_advantage: f64,
    pub games: Vec<GameSample>,
    #[serde(default = "yes")]
    pub new_known: bool,
}

fn yes() -> bool {
    true
}

/// How far a player's recent box worth, shrunk toward his entering worth by his minutes,
/// has moved from that worth. Nothing without minutes.
pub fn surprise(prior_elo: f64, recent_elo: f64, recent_minutes: f64, prior_minutes: f64) -> f64 {
    if recent_minutes <= 0.0 {
        return 0.0;
    }
    (recent_elo - prior_elo) * recent_minutes / (recent_minutes + prior_minutes)
}

/// The shift a floor gives its team: the damped sum of the surprises of the players the
/// population names.
pub fn team_shift(
    floor: &[Floor],
    prior_minutes: f64,
    damping: f64,
    population: Population,
) -> f64 {
    damping
        * floor
            .iter()
            .filter(|f| population.includes(f))
            .filter_map(|f| {
                f.recent
                    .map(|(elo, minutes)| surprise(f.prior_elo, elo, minutes, prior_minutes))
            })
            .sum::<f64>()
}

/// One game forecast four ways: the stored Elo, the Elo with the YAXEL shifts, and the
/// absence-penalized rating with tonight's missing Elo read from the previous game and
/// from tonight's box score (`Strength`); no rule is in the last two.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct GameScore {
    pub season: SeasonYear,
    pub game_date: NaiveDate,
    pub home_game_no: u32,
    pub away_game_no: u32,
    pub p_base: f64,
    pub p_yaxel: f64,
    pub p_penalized_last: f64,
    pub p_penalized_tonight: f64,
    pub home_won: bool,
    /// Each side had a new player on its floor at the cut's minutes in its previous game.
    pub home_new: bool,
    pub away_new: bool,
    /// The Elo each side was missing tonight, by its box score.
    pub home_missing_tonight: f64,
    pub away_missing_tonight: f64,
}

impl GameScore {
    /// A new player at the cut's minutes on either floor.
    pub fn new_floor(&self) -> bool {
        self.home_new || self.away_new
    }
}

/// The plain Elo forecast of a sample and the forecast with the shifts of `params`, whose
/// window is `window_index` in the sample's floors. `cut_minutes` is the new-player cut's
/// minutes, `None` for a season that cannot tell a new player, so no game of it is one with
/// a new player on a floor.
pub fn forecast(
    season: SeasonYear,
    sample: &GameSample,
    window_index: usize,
    params: &Params,
    home_advantage: f64,
    population: Population,
    cut_minutes: Option<f64>,
) -> GameScore {
    let bonus = if sample.neutral { 0.0 } else { home_advantage };
    let (home_floor, away_floor) = &sample.floors[window_index];
    let home_shift = team_shift(home_floor, params.prior_minutes, params.damping, population);
    let away_shift = team_shift(away_floor, params.prior_minutes, params.damping, population);
    let (home, away) = (&sample.home_strength, &sample.away_strength);
    GameScore {
        season,
        game_date: sample.game_date,
        home_game_no: sample.home_game_no,
        away_game_no: sample.away_game_no,
        p_base: elo::home_win_probability(sample.home_elo_pre, sample.away_elo_pre, bonus),
        p_yaxel: elo::home_win_probability(
            sample.home_elo_pre + home_shift,
            sample.away_elo_pre + away_shift,
            bonus,
        ),
        p_penalized_last: elo::home_win_probability(home.less_last(), away.less_last(), bonus),
        p_penalized_tonight: elo::home_win_probability(
            home.less_tonight(),
            away.less_tonight(),
            bonus,
        ),
        home_won: sample.home_won,
        home_new: cut_minutes.is_some_and(|m| has_new_player(home_floor, m)),
        away_new: cut_minutes.is_some_and(|m| has_new_player(away_floor, m)),
        home_missing_tonight: home.missing_tonight,
        away_missing_tonight: away.missing_tonight,
    }
}

/// The two forecasts scored over games: mean Brier score and log loss of each, the mean
/// gain (base Brier less YAXEL Brier, positive when YAXEL is the closer), its standard
/// error and the gain in standard errors.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Scored {
    pub games: u32,
    pub brier_base: f64,
    pub brier_yaxel: f64,
    pub log_loss_base: f64,
    pub log_loss_yaxel: f64,
    pub gain: f64,
    pub gain_se: Option<f64>,
    pub z: Option<f64>,
}

fn brier(p: f64, won: bool) -> f64 {
    (p - if won { 1.0 } else { 0.0 }).powi(2)
}

fn log_loss(p: f64, won: bool) -> f64 {
    let p = p.clamp(1e-6, 1.0 - 1e-6);
    -(if won { p.ln() } else { (1.0 - p).ln() })
}

/// Score forecasts; `None` without one.
pub fn score<'a>(scores: impl IntoIterator<Item = &'a GameScore>) -> Option<Scored> {
    let scores: Vec<&GameScore> = scores.into_iter().collect();
    if scores.is_empty() {
        return None;
    }
    let n = scores.len() as f64;
    let gains: Vec<f64> = scores
        .iter()
        .map(|s| brier(s.p_base, s.home_won) - brier(s.p_yaxel, s.home_won))
        .collect();
    let gain = gains.iter().sum::<f64>() / n;
    let sd = (scores.len() >= 2)
        .then(|| (gains.iter().map(|g| (g - gain).powi(2)).sum::<f64>() / (n - 1.0)).sqrt());
    let gain_se = sd.map(|sd| sd / n.sqrt());
    Some(Scored {
        games: scores.len() as u32,
        brier_base: scores
            .iter()
            .map(|s| brier(s.p_base, s.home_won))
            .sum::<f64>()
            / n,
        brier_yaxel: scores
            .iter()
            .map(|s| brier(s.p_yaxel, s.home_won))
            .sum::<f64>()
            / n,
        log_loss_base: scores
            .iter()
            .map(|s| log_loss(s.p_base, s.home_won))
            .sum::<f64>()
            / n,
        log_loss_yaxel: scores
            .iter()
            .map(|s| log_loss(s.p_yaxel, s.home_won))
            .sum::<f64>()
            / n,
        gain,
        gain_se,
        z: gain_se.filter(|se| *se > 0.0).map(|se| gain / se),
    })
}

/// The games in which neither team has played more than `through` games: the early season.
pub fn early(scores: &[GameScore], through: u32) -> impl Iterator<Item = &GameScore> {
    scores
        .iter()
        .filter(move |s| s.home_game_no <= through && s.away_game_no <= through)
}

/// The games with a new player at the cut's minutes on either floor.
pub fn with_new_floor<'a>(
    scores: impl IntoIterator<Item = &'a GameScore>,
) -> impl Iterator<Item = &'a GameScore> {
    scores.into_iter().filter(|s| s.new_floor())
}

/// The win chance one Elo buys at even odds: the slope of the logistic win probability at a
/// difference of zero, ln(10) / 1600, about 0.0014 a point of Elo, so a point of win chance
/// (a hundredth) is about 7 Elo.
pub const WIN_CHANCE_PER_ELO: f64 = std::f64::consts::LN_10 / 1600.0;

/// One forecast on the games with a new player on a floor: its Brier score on them, on the
/// rest and on all the games; and the bias of the new players' teams, the result of each
/// team that had the new player less the forecast's chance for it, so a positive bias says
/// those teams beat the rating, with its standard error and the bias in standard errors.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Baseline {
    pub brier_new: Option<f64>,
    pub brier_rest: Option<f64>,
    pub brier_all: Option<f64>,
    pub bias: Option<f64>,
    pub bias_se: Option<f64>,
    pub bias_z: Option<f64>,
}

impl Baseline {
    /// The bias as Elo at even odds.
    pub fn bias_elo(&self) -> Option<f64> {
        self.bias.map(|b| b / WIN_CHANCE_PER_ELO)
    }
}

/// The games with a new player on a floor against the rest, and the new players' teams
/// against three forecasts with no rule in them: Elo's own, and the absence-penalized
/// rating with tonight's missing Elo read from the previous game and from tonight's box
/// score. A game with a new player on both floors counts among the games and says nothing
/// about the bias: its two residuals would cancel.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Cut {
    /// Games with a new player on a floor, and the games in all.
    pub games: u32,
    pub of: u32,
    /// Team-games with a new player on the floor and none on the other: the bias is read
    /// over them.
    pub team_games: u32,
    /// The Elo missing tonight on average, by the box scores: over the team-games with a
    /// new player on the floor, and over the team-games without one.
    pub missing_new: Option<f64>,
    pub missing_rest: Option<f64>,
    pub elo: Baseline,
    pub penalized_last: Baseline,
    pub penalized_tonight: Baseline,
}

/// One forecast measured on the cut: `p` reads its chance of a home win from a game. The
/// team-games the bias is read over come with it.
fn baseline(scores: &[&GameScore], p: impl Fn(&GameScore) -> f64) -> (Baseline, u32) {
    let mean = |xs: &[f64]| (!xs.is_empty()).then(|| xs.iter().sum::<f64>() / xs.len() as f64);
    let (mut new, mut rest) = (Vec::new(), Vec::new());
    for s in scores {
        let b = brier(p(s), s.home_won);
        if s.new_floor() {
            new.push(b);
        } else {
            rest.push(b);
        }
    }
    let mut residuals = Vec::new();
    for s in scores {
        let home = if s.home_won { 1.0 } else { 0.0 };
        match (s.home_new, s.away_new) {
            (true, false) => residuals.push(home - p(s)),
            (false, true) => residuals.push((1.0 - home) - (1.0 - p(s))),
            _ => {}
        }
    }
    let bias = mean(&residuals);
    let bias_se = bias.filter(|_| residuals.len() >= 2).map(|b| {
        let n = residuals.len() as f64;
        (residuals.iter().map(|r| (r - b).powi(2)).sum::<f64>() / (n - 1.0)).sqrt() / n.sqrt()
    });
    let all: Vec<f64> = new.iter().chain(&rest).copied().collect();
    (
        Baseline {
            brier_new: mean(&new),
            brier_rest: mean(&rest),
            brier_all: mean(&all),
            bias,
            bias_se,
            bias_z: bias
                .zip(bias_se.filter(|se| *se > 0.0))
                .map(|(b, se)| b / se),
        },
        residuals.len() as u32,
    )
}

/// Cut the games by their floors and measure the three forecasts on them.
pub fn cut<'a>(scores: impl IntoIterator<Item = &'a GameScore>) -> Cut {
    let scores: Vec<&GameScore> = scores.into_iter().collect();
    let mean = |xs: &[f64]| (!xs.is_empty()).then(|| xs.iter().sum::<f64>() / xs.len() as f64);
    let (mut missing_new, mut missing_rest) = (Vec::new(), Vec::new());
    for s in &scores {
        for (new, missing) in [
            (s.home_new, s.home_missing_tonight),
            (s.away_new, s.away_missing_tonight),
        ] {
            if new {
                missing_new.push(missing);
            } else {
                missing_rest.push(missing);
            }
        }
    }
    let (elo, team_games) = baseline(&scores, |s| s.p_base);
    Cut {
        games: scores.iter().filter(|s| s.new_floor()).count() as u32,
        of: scores.len() as u32,
        team_games,
        missing_new: mean(&missing_new),
        missing_rest: mean(&missing_rest),
        elo,
        penalized_last: baseline(&scores, |s| s.p_penalized_last).0,
        penalized_tonight: baseline(&scores, |s| s.p_penalized_tonight).0,
    }
}

/// One fold: the parameters chosen on the training seasons, scored on the test seasons.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Fold {
    pub trained_on: Vec<SeasonYear>,
    pub tested_on: Vec<SeasonYear>,
    pub chosen: Params,
    pub train: Scored,
    pub test: Option<Scored>,
    pub test_early: Option<Scored>,
}

/// One season under its fold's parameters: all games, the early season, and the games
/// with a new player on a floor.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SeasonLine {
    pub season: SeasonYear,
    pub params: Params,
    pub all: Scored,
    pub early: Option<Scored>,
    pub new_floor: Option<Scored>,
}

/// The grid run for one population: the folds, every test season under its fold's
/// parameters, and the test seasons pooled, over all games, the early season and the games
/// with a new player on a floor.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Run {
    pub population: Population,
    pub folds: Vec<Fold>,
    pub per_season: Vec<SeasonLine>,
    pub pooled: Option<Scored>,
    pub pooled_early: Option<Scored>,
    pub pooled_new_floor: Option<Scored>,
    pub pooled_new_floor_early: Option<Scored>,
}

/// One season's new-player cut, all games and the early season; `None` for a season whose
/// new players cannot be told.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CutLine {
    pub season: SeasonYear,
    /// The season's games, and its early-season games.
    pub games: u32,
    pub early_games: u32,
    pub all: Option<Cut>,
    pub early: Option<Cut>,
}

/// The new players' mark on Elo's own forecasts, season by season and pooled over the
/// seasons that can tell them, before any rule: the premise the term rests on.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NewPlayers {
    /// The minutes in the team's previous game a new player needs to count.
    pub minutes: f64,
    pub per_season: Vec<CutLine>,
    /// The seasons whose new players cannot be told, left out of the pooled cut.
    pub not_told: Vec<SeasonYear>,
    pub pooled: Cut,
    pub pooled_early: Cut,
}

/// The backtest: the grid, the new players' mark on Elo, and the grid run twice, with the
/// term on every player and, over the seasons that can tell a new player, on the new players
/// alone (`None` with fewer than two such seasons).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Report {
    pub model: String,
    pub grid: Grid,
    pub through_game: u32,
    pub seasons: Vec<SeasonYear>,
    pub new_players: NewPlayers,
    pub all: Run,
    pub new: Option<Run>,
}

/// The forecasts of a season under `params`.
fn season_scores(
    sample: &SeasonSample,
    grid: &Grid,
    params: &Params,
    population: Population,
    cut_minutes: f64,
) -> Vec<GameScore> {
    let window_index = grid
        .window_days
        .iter()
        .position(|w| *w == params.window_days)
        .expect("the parameters come from the grid");
    sample
        .games
        .iter()
        .map(|g| {
            forecast(
                sample.season,
                g,
                window_index,
                params,
                sample.home_advantage,
                population,
                sample.new_known.then_some(cut_minutes),
            )
        })
        .collect()
}

/// Run the backtest over the samples: the seasons in order are split into two folds by
/// turns, each fold's parameters are the grid point with the greatest mean Brier gain over
/// all games of its training seasons, and the test seasons are scored under them, with the
/// term on every player and again on the new players alone. `new_minutes` is the minutes a
/// new player needs in his team's previous game for the game to count as one with a new
/// player on a floor. `None` with fewer than two seasons, an empty grid, or a sample whose
/// floors were not built for the grid's windows.
pub fn backtest(
    samples: &[SeasonSample],
    grid: &Grid,
    through_game: u32,
    new_minutes: f64,
) -> Option<Report> {
    let candidates = grid.params();
    if samples.len() < 2 || candidates.is_empty() {
        return None;
    }
    if samples.iter().any(|s| {
        s.games
            .iter()
            .any(|g| g.floors.len() != grid.window_days.len())
    }) {
        return None;
    }
    let mut samples: Vec<&SeasonSample> = samples.iter().collect();
    samples.sort_by_key(|s| s.season);
    // The new players' mark on Elo's own forecasts, which no parameter touches, over the
    // seasons that can tell a new player.
    let mut per_season = Vec::new();
    let mut pooled = Vec::new();
    for s in &samples {
        let scores = season_scores(s, grid, &candidates[0], Population::All, new_minutes);
        per_season.push(CutLine {
            season: s.season,
            games: scores.len() as u32,
            early_games: early(&scores, through_game).count() as u32,
            all: s.new_known.then(|| cut(&scores)),
            early: s.new_known.then(|| cut(early(&scores, through_game))),
        });
        if s.new_known {
            pooled.extend(scores);
        }
    }
    let new_players = NewPlayers {
        minutes: new_minutes,
        per_season,
        not_told: samples
            .iter()
            .filter(|s| !s.new_known)
            .map(|s| s.season)
            .collect(),
        pooled: cut(&pooled),
        pooled_early: cut(early(&pooled, through_game)),
    };
    let known: Vec<&SeasonSample> = samples.iter().copied().filter(|s| s.new_known).collect();
    Some(Report {
        model: MODEL_VERSION.to_string(),
        grid: grid.clone(),
        through_game,
        seasons: samples.iter().map(|s| s.season).collect(),
        new_players,
        all: run(
            &samples,
            grid,
            &candidates,
            through_game,
            Population::All,
            new_minutes,
        ),
        new: (known.len() >= 2).then(|| {
            run(
                &known,
                grid,
                &candidates,
                through_game,
                Population::New,
                new_minutes,
            )
        }),
    })
}

/// The grid run for one population over the sorted samples.
fn run(
    samples: &[&SeasonSample],
    grid: &Grid,
    candidates: &[Params],
    through_game: u32,
    population: Population,
    new_minutes: f64,
) -> Run {
    let mut folds = Vec::new();
    let mut per_season = Vec::new();
    let mut pooled_scores = Vec::new();
    for fold in 0..2 {
        let train: Vec<&SeasonSample> = samples
            .iter()
            .enumerate()
            .filter(|(i, _)| i % 2 != fold)
            .map(|(_, s)| *s)
            .collect();
        let test: Vec<&SeasonSample> = samples
            .iter()
            .enumerate()
            .filter(|(i, _)| i % 2 == fold)
            .map(|(_, s)| *s)
            .collect();
        let mut best: Option<(Params, Scored)> = None;
        for params in candidates {
            let scores: Vec<GameScore> = train
                .iter()
                .flat_map(|s| season_scores(s, grid, params, population, new_minutes))
                .collect();
            let Some(scored) = score(&scores) else {
                continue;
            };
            if best.as_ref().is_none_or(|(_, b)| scored.gain > b.gain) {
                best = Some((*params, scored));
            }
        }
        let Some((chosen, train_scored)) = best else {
            continue;
        };
        let mut test_scores = Vec::new();
        for s in &test {
            let scores = season_scores(s, grid, &chosen, population, new_minutes);
            if let Some(all) = score(&scores) {
                per_season.push(SeasonLine {
                    season: s.season,
                    params: chosen,
                    all,
                    early: score(early(&scores, through_game)),
                    new_floor: score(with_new_floor(&scores)),
                });
            }
            test_scores.extend(scores);
        }
        folds.push(Fold {
            trained_on: train.iter().map(|s| s.season).collect(),
            tested_on: test.iter().map(|s| s.season).collect(),
            chosen,
            train: train_scored,
            test: score(&test_scores),
            test_early: score(early(&test_scores, through_game)),
        });
        pooled_scores.extend(test_scores);
    }
    per_season.sort_by_key(|l| l.season);
    Run {
        population,
        folds,
        per_season,
        pooled: score(&pooled_scores),
        pooled_early: score(early(&pooled_scores, through_game)),
        pooled_new_floor: score(with_new_floor(&pooled_scores)),
        pooled_new_floor_early: score(with_new_floor(early(&pooled_scores, through_game))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    #[test]
    fn a_surprise_grows_with_minutes_and_a_shift_sums_the_floor() {
        // A rookie priced at -10 playing like +90: at 250 minutes against a 250-minute prior
        // the surprise is half the gap; at no minutes nothing.
        assert!((surprise(-10.0, 90.0, 250.0, 250.0) - 50.0).abs() < 1e-12);
        assert!((surprise(-10.0, 90.0, 750.0, 250.0) - 75.0).abs() < 1e-12);
        assert_eq!(surprise(-10.0, 90.0, 0.0, 250.0), 0.0);
        let floor = [
            Floor {
                prior_elo: -10.0,
                recent: Some((90.0, 250.0)),
                new: true,
                last_minutes: 30.0,
            },
            Floor {
                prior_elo: 40.0,
                recent: Some((20.0, 250.0)),
                new: false,
                last_minutes: 34.0,
            },
            Floor {
                prior_elo: 30.0,
                recent: None,
                new: false,
                last_minutes: 12.0,
            },
        ];
        // +50 and -10, damped by a half.
        assert!((team_shift(&floor, 250.0, 0.5, Population::All) - 20.0).abs() < 1e-12);
        // The new players alone: the rookie's +50.
        assert!((team_shift(&floor, 250.0, 0.5, Population::New) - 25.0).abs() < 1e-12);
        assert_eq!(team_shift(&[], 250.0, 1.0, Population::All), 0.0);
        assert!(has_new_player(&floor, 30.0) && !has_new_player(&floor, 30.5));
        assert!(
            !has_new_player(&floor[1..], 0.0),
            "no new player, whatever the minutes"
        );
    }

    fn sample(home_won: bool, home_floor: Vec<Floor>, game_no: u32) -> GameSample {
        GameSample {
            game_date: d("2026-11-01"),
            home_game_no: game_no,
            away_game_no: game_no,
            home_elo_pre: 1500.0,
            away_elo_pre: 1500.0,
            home_strength: Strength::plain(1500.0),
            away_strength: Strength::plain(1500.0),
            neutral: false,
            home_won,
            floors: vec![(home_floor, vec![])],
        }
    }

    #[test]
    fn the_forecast_moves_with_the_shift_and_the_score_measures_the_gain() {
        let grid = Grid {
            window_days: vec![14],
            prior_minutes: vec![250.0],
            dampings: vec![1.0],
        };
        let params = grid.params()[0];
        let star = Floor {
            prior_elo: 0.0,
            recent: Some((100.0, 250.0)),
            new: true,
            last_minutes: 30.0,
        };
        let g = sample(true, vec![star], 5);
        let s = forecast(
            SeasonYear::of(2027),
            &g,
            0,
            &params,
            0.0,
            Population::All,
            Some(24.0),
        );
        assert!(s.home_new && !s.away_new && s.new_floor());
        // A season that cannot tell a new player has no new-player floors.
        assert!(!forecast(
            SeasonYear::of(2027),
            &g,
            0,
            &params,
            0.0,
            Population::All,
            None
        )
        .new_floor());
        // A veteran's floor is no new-player floor, and the new population leaves him out.
        let vet = sample(true, vec![Floor { new: false, ..star }], 5);
        let v = forecast(
            SeasonYear::of(2027),
            &vet,
            0,
            &params,
            0.0,
            Population::New,
            Some(24.0),
        );
        assert!(!v.new_floor() && (v.p_yaxel - v.p_base).abs() < 1e-12);
        // Too few minutes in the previous game: on the floor, not in the cut.
        let bench = sample(
            true,
            vec![Floor {
                last_minutes: 10.0,
                ..star
            }],
            5,
        );
        assert!(!forecast(
            SeasonYear::of(2027),
            &bench,
            0,
            &params,
            0.0,
            Population::All,
            Some(24.0)
        )
        .new_floor());
        assert!((s.p_base - 0.5).abs() < 1e-12);
        // Nobody missing: the penalized forecasts are Elo's own.
        assert!((s.p_penalized_last - 0.5).abs() < 1e-12);
        assert!((s.p_penalized_tonight - 0.5).abs() < 1e-12);
        assert_eq!((s.home_missing_tonight, s.away_missing_tonight), (0.0, 0.0));
        // The home side 20 above its stored rating at full strength, missing 70 by its
        // previous game and 120 tonight: the penalized forecasts read 1450 and 1400 for it.
        let short = GameSample {
            home_strength: Strength {
                full_pre: 1520.0,
                missing_last: 70.0,
                missing_tonight: 120.0,
            },
            ..g.clone()
        };
        let p = forecast(
            SeasonYear::of(2027),
            &short,
            0,
            &params,
            0.0,
            Population::All,
            Some(24.0),
        );
        assert!((p.p_base - 0.5).abs() < 1e-12);
        assert!(
            (p.p_penalized_last - elo::home_win_probability(1450.0, 1500.0, 0.0)).abs() < 1e-12
        );
        assert!(
            (p.p_penalized_tonight - elo::home_win_probability(1400.0, 1500.0, 0.0)).abs() < 1e-12
        );
        assert_eq!(
            (p.home_missing_tonight, p.away_missing_tonight),
            (120.0, 0.0)
        );
        // +50 Elo for the home side: about 57%.
        assert!(s.p_yaxel > 0.57 && s.p_yaxel < 0.58, "{}", s.p_yaxel);
        let won = [s; 3];
        let sc = score(&won).unwrap();
        assert_eq!(sc.games, 3);
        assert!(sc.gain > 0.0 && sc.brier_yaxel < sc.brier_base);
        assert_eq!(
            (sc.gain_se, sc.z),
            (Some(0.0), None),
            "no spread in equal games"
        );
        assert!(score(std::iter::empty()).is_none());
        // The early filter keeps the games before the cut.
        let late = GameScore {
            home_game_no: 40,
            ..s
        };
        let all = vec![s, late];
        assert_eq!(early(&all, 30).count(), 1);
    }

    #[test]
    fn the_backtest_chooses_on_one_fold_and_scores_on_the_other() {
        let grid = Grid {
            window_days: vec![14],
            prior_minutes: vec![250.0],
            dampings: vec![0.0, 1.0],
        };
        let star = Floor {
            prior_elo: 0.0,
            recent: Some((100.0, 250.0)),
            new: true,
            last_minutes: 30.0,
        };
        // In the even seasons the home side with the emerging star wins, so the full
        // damping pays; in the odd seasons it loses, so the smaller damping hurts less.
        // Each fold must choose by its training seasons, which are the other parity, and
        // then be wrong on its test seasons.
        let season = |year: i32| SeasonSample {
            season: SeasonYear::of(year),
            home_advantage: 0.0,
            games: (1..=4)
                .map(|n| sample(year % 2 == 0, vec![star], n))
                .collect(),
            new_known: true,
        };
        let samples: Vec<SeasonSample> = (2020..2024).map(season).collect();
        let grid = Grid {
            dampings: vec![0.5, 1.0],
            ..grid
        };
        let r = backtest(&samples, &grid, 30, 24.0).unwrap();
        assert_eq!(r.all.folds.len(), 2);
        for f in &r.all.folds {
            assert_eq!(f.trained_on.len() + f.tested_on.len(), 4);
            let trained_on_wins = f.trained_on.iter().all(|s| i32::from(*s) % 2 == 0);
            if trained_on_wins {
                assert_eq!(f.chosen.damping, 1.0, "{f:?}");
                assert!(f.test.as_ref().unwrap().gain < 0.0, "{f:?}");
            } else {
                assert_eq!(f.chosen.damping, 0.5, "{f:?}");
                assert!(f.test.as_ref().unwrap().gain > 0.0, "{f:?}");
            }
        }
        assert_eq!(r.all.per_season.len(), 4);
        assert_eq!(r.all.pooled.as_ref().unwrap().games, 16);
        assert_eq!(r.all.pooled_early.as_ref().unwrap().games, 16);
        // Every floor here carries the new star, so the new-player games are all of them and
        // the run on the new players alone is the same run.
        assert_eq!(r.all.pooled_new_floor.as_ref().unwrap().games, 16);
        let new = r.new.as_ref().unwrap();
        assert_eq!(new.population, Population::New);
        assert_eq!(new.pooled, r.all.pooled);
        assert_eq!(r.new_players.pooled.games, 16);
        assert_eq!(r.new_players.per_season.len(), 4);
        assert!(r.new_players.not_told.is_empty());
        // A season that cannot tell its new players is listed, left out of the pooled cut,
        // and out of the run on the new players alone; with one such season left over, that
        // run does not happen.
        let mut blind = samples.clone();
        for s in &mut blind[..3] {
            s.new_known = false;
        }
        let r = backtest(&blind, &grid, 30, 24.0).unwrap();
        assert_eq!(r.new_players.not_told.len(), 3);
        assert_eq!(r.new_players.pooled.games, 4);
        assert!(r.new_players.per_season[0].all.is_none());
        assert_eq!(r.new_players.per_season[0].games, 4);
        assert_eq!(r.new_players.per_season[0].early_games, 4);
        assert!(r.new.is_none());
        assert_eq!(
            r.all.pooled.as_ref().unwrap().games,
            16,
            "the all-player run keeps every season"
        );
        assert_eq!(
            r.all.pooled_new_floor.as_ref().unwrap().games,
            4,
            "but its new-player cut holds the season that can tell one alone"
        );
        assert!(backtest(&samples[..1], &grid, 30, 24.0).is_none());
        // A sample built for other windows is refused.
        let other = Grid {
            window_days: vec![7, 14],
            ..grid.clone()
        };
        assert!(backtest(&samples, &other, 30, 24.0).is_none());
    }

    #[test]
    fn the_cut_measures_elo_on_the_new_player_games_and_the_bias_of_their_teams() {
        let g = |p_base: f64, home_won: bool, home_new: bool, away_new: bool| GameScore {
            season: SeasonYear::of(2027),
            game_date: d("2026-11-01"),
            home_game_no: 1,
            away_game_no: 1,
            p_base,
            p_yaxel: p_base,
            // The penalized ratings read the home side 0.1 lower by its previous game and
            // 0.2 lower by tonight's box score, where the home side is missing 50 Elo.
            p_penalized_last: p_base - 0.1,
            p_penalized_tonight: p_base - 0.2,
            home_won,
            home_new,
            away_new,
            home_missing_tonight: 50.0,
            away_missing_tonight: 0.0,
        };
        // Two games with the home side's rookie: both won at 60%, residual +0.4 each. One
        // with the away side's rookie, lost by the away side at a 30% chance: residual
        // -0.3. One game without a new player, a 50-50 the home side lost. One with a new
        // player on both floors: among the games, out of the bias.
        let scores = vec![
            g(0.6, true, true, false),
            g(0.6, true, true, false),
            g(0.7, true, false, true),
            g(0.5, false, false, false),
            g(0.5, true, true, true),
        ];
        let c = cut(&scores);
        assert_eq!((c.games, c.of, c.team_games), (4, 5, 3));
        // Brier on the new-player games: (0.16 + 0.16 + 0.09 + 0.25) / 4; on the rest 0.25.
        assert!((c.elo.brier_new.unwrap() - 0.66 / 4.0).abs() < 1e-12);
        assert!((c.elo.brier_rest.unwrap() - 0.25).abs() < 1e-12);
        assert!((c.elo.brier_all.unwrap() - 0.91 / 5.0).abs() < 1e-12);
        // Bias: (0.4 + 0.4 - 0.3) / 3.
        let bias = c.elo.bias.unwrap();
        assert!((bias - 0.5 / 3.0).abs() < 1e-12);
        assert!(c.elo.bias_se.unwrap() > 0.0 && c.elo.bias_z.unwrap() > 0.0);
        // A sixth of a win at even odds is about 116 Elo.
        assert!((c.elo.bias_elo().unwrap() - bias / WIN_CHANCE_PER_ELO).abs() < 1e-9);
        assert!(c.elo.bias_elo().unwrap() > 110.0 && c.elo.bias_elo().unwrap() < 120.0);
        // Against the penalized ratings the home rookie's team beats its chance by 0.1
        // more, and the away rookie's team (whose chance rose by 0.1) by 0.1 less: the
        // bias moves by (0.1 + 0.1 - 0.1) / 3 per tenth of penalty.
        assert!((c.penalized_last.bias.unwrap() - 0.6 / 3.0).abs() < 1e-12);
        assert!((c.penalized_tonight.bias.unwrap() - 0.7 / 3.0).abs() < 1e-12);
        assert!(c.penalized_tonight.brier_all.unwrap() > c.elo.brier_all.unwrap());
        // The missing Elo tonight: the five team-games with a rookie on the floor are three
        // home sides at 50 and two away sides at 0, a mean of 30; the five without are two
        // home sides at 50 and three away sides at 0, a mean of 20.
        assert!((c.missing_new.unwrap() - 30.0).abs() < 1e-12);
        assert!((c.missing_rest.unwrap() - 20.0).abs() < 1e-12);
        // Nothing in the cut: no means, no bias.
        let none = cut(&scores[3..4]);
        assert_eq!((none.games, none.team_games), (0, 0));
        assert_eq!((none.elo.brier_new, none.elo.bias), (None, None));
        assert_eq!(none.missing_new, None);
        assert_eq!(with_new_floor(&scores).count(), 4);
    }

    fn game(id: i64, date: &str, home: i32, away: i32, score: (i32, i32)) -> Game {
        use crate::domain::models::{EloSource, GameType, ResultSource};
        Game {
            id,
            season: SeasonYear::of(2027),
            game_date: d(date),
            seq: 0,
            home_team_id: TeamId::new(home),
            away_team_id: TeamId::new(away),
            neutral: false,
            game_type: GameType::Regular,
            home_score: Some(score.0),
            away_score: Some(score.1),
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
            cup_stage: None,
        }
    }

    fn line(game_id: i64, player: i32, team: i32, seconds: i32) -> BoxLine {
        BoxLine {
            game_id,
            player_id: PlayerId::new(player),
            team_id: TeamId::new(team),
            seconds: Some(seconds),
            pts: Some(0),
            reb: Some(0),
            oreb: Some(0),
            dreb: Some(0),
            ast: Some(0),
            stl: Some(0),
            blk: Some(0),
            tov: Some(0),
            pf: Some(0),
            fgm: Some(0),
            fga: Some(0),
            fg3m: Some(0),
            ftm: Some(0),
            fta: Some(0),
        }
    }

    #[test]
    fn the_absences_are_read_from_the_box_scores_and_the_strengths_from_the_replay() {
        // Teams 1 and 2 meet six times. Team 1: players 11 and 12 (worth 60 and 30), 13
        // (worth 40) who is traded to team 2 after the first game, 14 (worth 20) who ended
        // the season before with team 1, is out from the first game and first plays in the
        // third, 19 (worth 35) who is signed in season and also first plays in the third,
        // and four players without a worth (15 to 18) who always play. Team 2: 21 (worth
        // 50), 22 (no worth) and four without a worth (23 to 26) who always play.
        let t1 = TeamId::new(1);
        let t2 = TeamId::new(2);
        let carried: HashMap<PlayerId, TeamId> = [(PlayerId::new(14), t1)].into_iter().collect();
        let games = vec![
            game(1, "2026-10-22", 1, 2, (100, 90)),
            game(2, "2026-10-25", 2, 1, (95, 101)),
            game(3, "2026-10-28", 1, 2, (99, 100)),
            game(4, "2026-11-02", 2, 1, (99, 100)),
            game(5, "2026-11-04", 1, 2, (99, 100)),
            game(6, "2026-11-06", 2, 1, (99, 100)),
        ];
        let mut lines = vec![
            // Game 1: everyone but 14 plays; 13 for team 1.
            line(1, 11, 1, 2000),
            line(1, 12, 1, 1800),
            line(1, 13, 1, 1500),
            line(1, 21, 2, 2100),
            line(1, 22, 2, 1700),
            // Game 2: 12 dresses and does not play; 13 plays for team 2; 21 sits.
            line(2, 11, 1, 2000),
            line(2, 12, 1, 0),
            line(2, 13, 2, 1500),
            line(2, 22, 2, 1700),
            // Game 3: 12 is back, 14 and 19 play their first games; 21 still out; 13 sits.
            line(3, 11, 1, 2000),
            line(3, 12, 1, 1800),
            line(3, 14, 1, 1200),
            line(3, 19, 1, 1000),
            line(3, 22, 2, 1700),
            // Game 4: team 1 has no lines at all; team 2 has 21 back, 13 playing, 22 out.
            line(4, 21, 2, 2000),
            line(4, 13, 2, 1500),
            // Game 5: team 1's box score holds four players only, so it is not known
            // whole; team 2 is whole but for 13.
            line(5, 11, 1, 2000),
            line(5, 12, 1, 1800),
            line(5, 14, 1, 1200),
            line(5, 19, 1, 1000),
            line(5, 21, 2, 2000),
            line(5, 22, 2, 1700),
            // Game 6: everyone plays.
            line(6, 11, 1, 2000),
            line(6, 12, 1, 1800),
            line(6, 14, 1, 1200),
            line(6, 19, 1, 1000),
            line(6, 13, 2, 1500),
            line(6, 21, 2, 2000),
            line(6, 22, 2, 1700),
        ];
        for g in [1, 2, 3, 6] {
            for p in 15..=18 {
                lines.push(line(g, p, 1, 1800));
            }
        }
        for g in [1, 2, 3, 4, 5, 6] {
            for p in 23..=26 {
                lines.push(line(g, p, 2, 1800));
            }
        }
        let worth: HashMap<PlayerId, (f64, f64)> = [
            (PlayerId::new(11), (60.0, 0.0)),
            (PlayerId::new(12), (30.0, 0.0)),
            (PlayerId::new(13), (40.0, 0.0)),
            (PlayerId::new(14), (20.0, 0.0)),
            (PlayerId::new(19), (35.0, 0.0)),
            (PlayerId::new(21), (50.0, 0.0)),
        ]
        .into_iter()
        .collect();
        let a = box_absences(&games, &lines, &worth, &carried);
        // Game 1: 14 ended the season before with team 1, so he is on its roster from the
        // season's start, and he sat: 20. 19, signed in season, is not on it until his
        // first game. Team 2 is whole. No previous game, so nothing is known before tip-off.
        assert_eq!(a.tonight.get(&(1, t1)), Some(&20.0));
        assert_eq!(a.tonight.get(&(1, t2)), None);
        assert!(!a.last.contains_key(&(1, t1)) && !a.last.contains_key(&(1, t2)));
        // Game 2: 12 and 14 sat for team 1 (50); 13 plays for team 2 today, so his stint
        // with team 1 ended the day before and he is not missing from it; 19 is still off
        // the roster. Team 2 is missing 21 (50). By its previous game team 1 was missing 14
        // (20); team 2 nobody, and 13, not on its roster at the previous game, does not
        // read as having sat it.
        assert_eq!(a.tonight.get(&(2, t1)), Some(&50.0));
        assert_eq!(a.tonight.get(&(2, t2)), Some(&50.0));
        assert_eq!(a.last.get(&(2, t1)), Some(&20.0));
        assert_eq!(a.last.get(&(2, t2)), None);
        // Game 3: team 1 is whole tonight and was missing 12 and 14 by its previous game
        // (50). Team 2 is missing 21 and 13 tonight (90); by its previous game 21 (50).
        assert_eq!(a.tonight.get(&(3, t1)), None);
        assert_eq!(a.last.get(&(3, t1)), Some(&50.0));
        assert_eq!(a.tonight.get(&(3, t2)), Some(&90.0));
        assert_eq!(a.last.get(&(3, t2)), Some(&50.0));
        // Game 4: team 1 has no lines, so nothing is read for it tonight, and its previous
        // game tells nobody sat. Team 2 is missing 22 tonight, who has no worth, and was
        // missing 90 by its previous game.
        assert_eq!(a.tonight.get(&(4, t1)), None);
        assert_eq!(a.last.get(&(4, t1)), None);
        assert_eq!(a.tonight.get(&(4, t2)), None);
        assert_eq!(a.last.get(&(4, t2)), Some(&90.0));
        // Game 5: team 1's box score is not known whole, so the four who have no line do
        // not read as sitting, and its previous game (without lines) tells nothing. Team 2
        // is missing 13 tonight (40); by its previous game 22, who prices nothing.
        assert_eq!(a.tonight.get(&(5, t1)), None);
        assert_eq!(a.last.get(&(5, t1)), None);
        assert_eq!(a.tonight.get(&(5, t2)), Some(&40.0));
        assert_eq!(a.last.get(&(5, t2)), None);
        // Game 6: everyone plays. Team 1's previous box score was not known whole, so
        // nothing is known for it; team 2 was missing 13 by its previous game.
        assert_eq!(a.tonight.get(&(6, t1)), None);
        assert_eq!(a.tonight.get(&(6, t2)), None);
        assert_eq!(a.last.get(&(6, t1)), None);
        assert_eq!(a.last.get(&(6, t2)), Some(&40.0));
        // The strengths: the stored ratings here are flat 1500s, not a chain, so the
        // full-strength rating is the stored one moved by the replay's gap alone. Team 1
        // won game 1 at home without 14, so it gains more than the stored chain gave it:
        // before game 2 it stands above its stored rating and team 2 below, and the gaps
        // are zero-sum. It then won game 2 short-handed against a side as short, and the
        // gaps hold their signs before game 3.
        let s = strengths(&games, &a);
        assert_eq!(s.len(), 6);
        let (home1, away1) = s[&1];
        assert_eq!(
            home1,
            Strength {
                full_pre: 1500.0,
                missing_last: 0.0,
                missing_tonight: 20.0,
            }
        );
        assert_eq!(away1, Strength::plain(1500.0));
        let (home2, away2) = s[&2];
        assert!(home2.full_pre < 1500.0 && away2.full_pre > 1500.0, "{s:?}");
        assert!((home2.full_pre - 1500.0 + (away2.full_pre - 1500.0)).abs() < 1e-9);
        assert_eq!((home2.missing_last, home2.missing_tonight), (0.0, 50.0));
        assert_eq!((away2.missing_last, away2.missing_tonight), (20.0, 50.0));
        let (home3, away3) = s[&3];
        assert!(home3.full_pre > 1500.0 && away3.full_pre < 1500.0, "{s:?}");
        assert_eq!((home3.missing_last, home3.missing_tonight), (50.0, 0.0));
        assert_eq!((away3.missing_last, away3.missing_tonight), (50.0, 90.0));
        assert!((home3.less_last() - (home3.full_pre - 50.0)).abs() < 1e-12);
        assert!((away3.less_tonight() - (away3.full_pre - 90.0)).abs() < 1e-12);
        // A season without lines has no absences and the stored ratings stand.
        let none = strengths(&games, &box_absences(&games, &[], &worth, &carried));
        assert!(none
            .values()
            .all(|(h, a)| *h == Strength::plain(1500.0) && *a == Strength::plain(1500.0)));
        // The teams the players ended this season with: 13 with team 2, 11 with team 1; a
        // line without minutes (12 in game 2) does not move him, and a line of a game not
        // among the season's is ignored.
        let mut more = lines.clone();
        more.push(line(99, 11, 2, 2000));
        let ended = last_teams(&games, &more);
        assert_eq!(ended.get(&PlayerId::new(13)), Some(&t2));
        assert_eq!(ended.get(&PlayerId::new(11)), Some(&t1));
        assert_eq!(ended.get(&PlayerId::new(12)), Some(&t1));
        assert_eq!(ended.get(&PlayerId::new(99)), None);
    }

    #[test]
    fn a_stint_runs_from_the_first_game_or_the_seasons_start_to_the_day_before_the_next_team() {
        let (t, u) = (TeamId::new(1), TeamId::new(2));
        // Given out of order: two games for t, one for u, one for t again.
        let played = [
            (d("2026-10-20"), t),
            (d("2026-10-01"), t),
            (d("2026-10-09"), u),
            (d("2026-10-05"), t),
        ];
        let stint = |team, from: &str, to: &str| Stint {
            team,
            from: d(from),
            to: d(to),
        };
        // He ended the season before with t: on its roster from the start; u's stint
        // starts at his first game for u, and t's second stint at his first game back, so
        // the days between belong to the team he left.
        let s = stints(&played, Some(t));
        assert_eq!(
            s,
            vec![
                Stint {
                    team: t,
                    from: NaiveDate::MIN,
                    to: d("2026-10-08"),
                },
                stint(u, "2026-10-09", "2026-10-19"),
                Stint {
                    team: t,
                    from: d("2026-10-20"),
                    to: NaiveDate::MAX,
                },
            ]
        );
        // Without a team from the season before, or with another, the first stint starts
        // at his first game.
        assert_eq!(
            stints(&played, None)[0],
            stint(t, "2026-10-01", "2026-10-08")
        );
        assert_eq!(
            stints(&played, Some(u))[0],
            stint(t, "2026-10-01", "2026-10-08")
        );
        assert!(stints(&[], Some(t)).is_empty());
    }
}
