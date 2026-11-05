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

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

use crate::domain::elo;
use crate::domain::ids::SeasonYear;

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

/// One regular-season game as the backtest sees it before tip-off: the stored pre-game
/// ratings, the result, each team's game number in the season, and per window of the grid
/// (in the grid's order) the two floors.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GameSample {
    pub game_date: NaiveDate,
    pub home_game_no: u32,
    pub away_game_no: u32,
    pub home_elo_pre: f64,
    pub away_elo_pre: f64,
    pub neutral: bool,
    pub home_won: bool,
    pub floors: Vec<(Vec<Floor>, Vec<Floor>)>,
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

/// One game forecast two ways.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct GameScore {
    pub season: SeasonYear,
    pub game_date: NaiveDate,
    pub home_game_no: u32,
    pub away_game_no: u32,
    pub p_base: f64,
    pub p_yaxel: f64,
    pub home_won: bool,
    /// Each side had a new player on its floor at the cut's minutes in its previous game.
    pub home_new: bool,
    pub away_new: bool,
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
        home_won: sample.home_won,
        home_new: cut_minutes.is_some_and(|m| has_new_player(home_floor, m)),
        away_new: cut_minutes.is_some_and(|m| has_new_player(away_floor, m)),
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

/// Elo's own forecast on the games with a new player on a floor, against the rest, and the
/// bias: the result of each team that had the new player, less Elo's chance for it, so a
/// positive bias says those teams beat the rating. A game with a new player on both floors
/// counts among the games and says nothing about the bias: its two residuals would cancel.
/// No rule is applied here.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Cut {
    /// Games with a new player on a floor, and the games in all.
    pub games: u32,
    pub of: u32,
    pub brier_new: Option<f64>,
    pub brier_rest: Option<f64>,
    /// Team-games with a new player on the floor and none on the other, and their mean
    /// result less Elo's chance, with its standard error and the bias in standard errors.
    pub team_games: u32,
    pub bias: Option<f64>,
    pub bias_se: Option<f64>,
    pub bias_z: Option<f64>,
}

impl Cut {
    /// The bias as Elo at even odds.
    pub fn bias_elo(&self) -> Option<f64> {
        self.bias.map(|b| b / WIN_CHANCE_PER_ELO)
    }
}

/// Cut the games by their floors and measure Elo on them.
pub fn cut<'a>(scores: impl IntoIterator<Item = &'a GameScore>) -> Cut {
    let scores: Vec<&GameScore> = scores.into_iter().collect();
    let mean = |xs: &[f64]| (!xs.is_empty()).then(|| xs.iter().sum::<f64>() / xs.len() as f64);
    let (new, rest): (Vec<_>, Vec<_>) = scores.iter().partition(|s| s.new_floor());
    let brier_of =
        |g: &[&&GameScore]| -> Vec<f64> { g.iter().map(|s| brier(s.p_base, s.home_won)).collect() };
    let mut residuals = Vec::new();
    for s in &scores {
        let home = if s.home_won { 1.0 } else { 0.0 };
        match (s.home_new, s.away_new) {
            (true, false) => residuals.push(home - s.p_base),
            (false, true) => residuals.push((1.0 - home) - (1.0 - s.p_base)),
            _ => {}
        }
    }
    let bias = mean(&residuals);
    let bias_se = bias.filter(|_| residuals.len() >= 2).map(|b| {
        let n = residuals.len() as f64;
        (residuals.iter().map(|r| (r - b).powi(2)).sum::<f64>() / (n - 1.0)).sqrt() / n.sqrt()
    });
    Cut {
        games: new.len() as u32,
        of: scores.len() as u32,
        brier_new: mean(&brier_of(&new)),
        brier_rest: mean(&brier_of(&rest)),
        team_games: residuals.len() as u32,
        bias,
        bias_se,
        bias_z: bias
            .zip(bias_se.filter(|se| *se > 0.0))
            .map(|(b, se)| b / se),
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
            home_won,
            home_new,
            away_new,
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
        assert!((c.brier_new.unwrap() - 0.66 / 4.0).abs() < 1e-12);
        assert!((c.brier_rest.unwrap() - 0.25).abs() < 1e-12);
        // Bias: (0.4 + 0.4 - 0.3) / 3.
        let bias = c.bias.unwrap();
        assert!((bias - 0.5 / 3.0).abs() < 1e-12);
        assert!(c.bias_se.unwrap() > 0.0 && c.bias_z.unwrap() > 0.0);
        // A sixth of a win at even odds is about 116 Elo.
        assert!((c.bias_elo().unwrap() - bias / WIN_CHANCE_PER_ELO).abs() < 1e-9);
        assert!(c.bias_elo().unwrap() > 110.0 && c.bias_elo().unwrap() < 120.0);
        // Nothing in the cut: no means, no bias.
        let none = cut(&scores[3..4]);
        assert_eq!((none.games, none.team_games), (0, 0));
        assert_eq!((none.brier_new, none.bias), (None, None));
        assert_eq!(with_new_floor(&scores).count(), 4);
    }
}
