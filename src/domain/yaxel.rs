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
/// and his box worth and minutes over the window before the game, when he has any.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Floor {
    pub prior_elo: f64,
    pub recent: Option<(f64, f64)>,
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

/// A season's samples with the home bonus its forecasts use.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SeasonSample {
    pub season: SeasonYear,
    pub home_advantage: f64,
    pub games: Vec<GameSample>,
}

/// How far a player's recent box worth, shrunk toward his entering worth by his minutes,
/// has moved from that worth. Nothing without minutes.
pub fn surprise(prior_elo: f64, recent_elo: f64, recent_minutes: f64, prior_minutes: f64) -> f64 {
    if recent_minutes <= 0.0 {
        return 0.0;
    }
    (recent_elo - prior_elo) * recent_minutes / (recent_minutes + prior_minutes)
}

/// The shift a floor gives its team: the damped sum of its players' surprises.
pub fn team_shift(floor: &[Floor], prior_minutes: f64, damping: f64) -> f64 {
    damping
        * floor
            .iter()
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
}

/// The plain Elo forecast of a sample and the forecast with the shifts of `params`, whose
/// window is `window_index` in the sample's floors.
pub fn forecast(
    season: SeasonYear,
    sample: &GameSample,
    window_index: usize,
    params: &Params,
    home_advantage: f64,
) -> GameScore {
    let bonus = if sample.neutral { 0.0 } else { home_advantage };
    let (home_floor, away_floor) = &sample.floors[window_index];
    let home_shift = team_shift(home_floor, params.prior_minutes, params.damping);
    let away_shift = team_shift(away_floor, params.prior_minutes, params.damping);
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

/// One season under its fold's parameters.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SeasonLine {
    pub season: SeasonYear,
    pub params: Params,
    pub all: Scored,
    pub early: Option<Scored>,
}

/// The backtest: the grid, the folds, every test season under its fold's parameters, and
/// the test seasons pooled.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Report {
    pub model: String,
    pub grid: Grid,
    pub through_game: u32,
    pub seasons: Vec<SeasonYear>,
    pub folds: Vec<Fold>,
    pub per_season: Vec<SeasonLine>,
    pub pooled: Option<Scored>,
    pub pooled_early: Option<Scored>,
}

/// The forecasts of a season under `params`.
fn season_scores(sample: &SeasonSample, grid: &Grid, params: &Params) -> Vec<GameScore> {
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
            )
        })
        .collect()
}

/// Run the backtest over the samples: the seasons in order are split into two folds by
/// turns, each fold's parameters are the grid point with the greatest mean Brier gain over
/// all games of its training seasons, and the test seasons are scored under them. `None`
/// with fewer than two seasons, an empty grid, or a sample whose floors were not built for
/// the grid's windows.
pub fn backtest(samples: &[SeasonSample], grid: &Grid, through_game: u32) -> Option<Report> {
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
        for params in &candidates {
            let scores: Vec<GameScore> = train
                .iter()
                .flat_map(|s| season_scores(s, grid, params))
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
            let scores = season_scores(s, grid, &chosen);
            if let Some(all) = score(&scores) {
                per_season.push(SeasonLine {
                    season: s.season,
                    params: chosen,
                    all,
                    early: score(early(&scores, through_game)),
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
    Some(Report {
        model: MODEL_VERSION.to_string(),
        grid: grid.clone(),
        through_game,
        seasons: samples.iter().map(|s| s.season).collect(),
        folds,
        per_season,
        pooled: score(&pooled_scores),
        pooled_early: score(early(&pooled_scores, through_game)),
    })
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
            },
            Floor {
                prior_elo: 40.0,
                recent: Some((20.0, 250.0)),
            },
            Floor {
                prior_elo: 30.0,
                recent: None,
            },
        ];
        // +50 and -10, damped by a half.
        assert!((team_shift(&floor, 250.0, 0.5) - 20.0).abs() < 1e-12);
        assert_eq!(team_shift(&[], 250.0, 1.0), 0.0);
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
        };
        let g = sample(true, vec![star], 5);
        let s = forecast(SeasonYear::of(2027), &g, 0, &params, 0.0);
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
        };
        let samples: Vec<SeasonSample> = (2020..2024).map(season).collect();
        let grid = Grid {
            dampings: vec![0.5, 1.0],
            ..grid
        };
        let r = backtest(&samples, &grid, 30).unwrap();
        assert_eq!(r.folds.len(), 2);
        for f in &r.folds {
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
        assert_eq!(r.per_season.len(), 4);
        assert_eq!(r.pooled.unwrap().games, 16);
        assert_eq!(r.pooled_early.unwrap().games, 16);
        assert!(backtest(&samples[..1], &grid, 30).is_none());
        // A sample built for other windows is refused.
        let other = Grid {
            window_days: vec![7, 14],
            ..grid.clone()
        };
        assert!(backtest(&samples, &other, 30).is_none());
    }
}
