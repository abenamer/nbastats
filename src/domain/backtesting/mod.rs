//! Historical validation: leakage-free pre-playoff forecasts, scoring rules, calibration and
//! baselines, with chronological rolling tuning of the Elo parameters.

pub mod metrics;

use std::collections::BTreeMap;

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

use crate::domain::ids::{SeasonYear, TeamId};
use crate::domain::models::{PlayoffResult, Round};
use crate::domain::playoffs::bracket::{Bracket, ModelSpec};
use crate::domain::validation::{invalid, ValidationError};

pub const MODEL_VERSION: &str = "backtest-1.1";

/// What the rolling tuning minimises on the earlier seasons.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TuningCriterion {
    /// The log loss of the played series: fifteen trials a season.
    #[default]
    Series,
    /// The log loss of the champion: one trial a season.
    Champion,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TuningGrid {
    pub home_advantages: Vec<f64>,
    pub scales: Vec<f64>,
    /// Minimum number of earlier seasons before a season is evaluated with tuned parameters.
    pub min_train_seasons: usize,
    #[serde(default)]
    pub criterion: TuningCriterion,
}

impl Default for TuningGrid {
    fn default() -> Self {
        Self {
            home_advantages: vec![0.0, 50.0, 100.0, 150.0],
            scales: vec![0.75, 1.0, 1.25, 1.5, 1.75],
            min_train_seasons: 5,
            criterion: TuningCriterion::Series,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BacktestParams {
    pub from_season: SeasonYear,
    pub to_season: SeasonYear,
    #[serde(default = "default_models")]
    pub models: Vec<ModelSpec>,
    #[serde(default = "default_tuning")]
    pub tuning: Option<TuningGrid>,
}

fn default_models() -> Vec<ModelSpec> {
    vec![
        ModelSpec::default(),
        ModelSpec::Equal,
        ModelSpec::WinPct,
        ModelSpec::Seed { p_better: 0.75 },
    ]
}
fn default_tuning() -> Option<TuningGrid> {
    Some(TuningGrid::default())
}

impl BacktestParams {
    pub fn validate(&self) -> Result<(), ValidationError> {
        if self.from_season.end_year() < 1984 || self.to_season < self.from_season {
            return invalid(
                "from_season",
                "season range must start in 1983-84 or later and end after it starts",
            );
        }
        if self.models.is_empty() || self.models.len() > 12 {
            return invalid("models", "between 1 and 12 models are allowed");
        }
        for m in &self.models {
            m.validate()?;
            if !m.is_stateless() {
                return invalid(
                    "models",
                    "backtest models must keep their ratings fixed; omit the K-factor",
                );
            }
        }
        if let Some(t) = &self.tuning {
            if t.home_advantages.is_empty()
                || t.scales.is_empty()
                || t.home_advantages.len() * t.scales.len() > 400
            {
                return invalid("tuning", "tuning grid must have between 1 and 400 points");
            }
        }
        Ok(())
    }
}

/// One season's bracket together with the information cutoff used to build it.
pub struct SeasonInput {
    pub season: SeasonYear,
    pub bracket: Bracket,
    pub cutoff: NaiveDate,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamForecastRow {
    pub season: SeasonYear,
    pub team_id: TeamId,
    pub code: String,
    pub franchise_key: String,
    pub seed: u8,
    pub probs: [f64; 4],
    pub actual: PlayoffResult,
    pub champion: bool,
}

/// One played series scored against the model's probability for its two participants, with
/// the series' actual home court.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SeriesScore {
    pub season: SeasonYear,
    pub round: Round,
    pub winner_id: TeamId,
    pub loser_id: TeamId,
    /// Team codes, for display.
    pub winner: String,
    pub loser: String,
    /// Games won by the winner and by the loser.
    pub games: (u8, u8),
    pub p_winner: f64,
    /// (p_winner − 1)², the two-sided Brier score.
    pub brier: f64,
    pub log_loss: f64,
    /// Whether the model's favourite won; `None` when the model had none (one half each).
    pub hit: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SeasonScore {
    pub season: SeasonYear,
    pub champion: String,
    pub favorite: String,
    pub favorite_prob: f64,
    pub p_champion: f64,
    pub brier: f64,
    pub log_loss: f64,
    pub hit: bool,
    /// Played series of the season that were scored.
    #[serde(default)]
    pub series: usize,
    /// Mean two-sided Brier score over the season's series.
    #[serde(default)]
    pub series_brier: f64,
    /// Mean log loss over the season's series.
    #[serde(default)]
    pub series_log_loss: f64,
    /// Series the model's favourite won.
    #[serde(default)]
    pub series_hits: usize,
    /// Series in which the model had a favourite.
    #[serde(default)]
    pub series_with_favourite: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelReport {
    pub name: String,
    pub spec: Option<ModelSpec>,
    pub seasons: Vec<SeasonScore>,
    pub summary: metrics::Summary,
    /// The same forecasts scored series by series (a result stored before series scoring
    /// reads as empty).
    #[serde(default)]
    pub series_summary: metrics::SeriesSummary,
    /// This model's series scores against the first model's, paired series by series.
    #[serde(default)]
    pub series_vs_first: Option<metrics::PairedDifference>,
    /// Calibration by stage reached, and under `series` the favourite's series probability.
    pub calibration: BTreeMap<String, Vec<metrics::CalibrationBin>>,
    pub forecasts: Vec<TeamForecastRow>,
    #[serde(default)]
    pub series: Vec<SeriesScore>,
    /// For the rolling-tuned model: the parameters chosen for each evaluated season.
    pub chosen_params: Option<Vec<(SeasonYear, f64, f64)>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BacktestResult {
    pub model_version: String,
    pub from_season: SeasonYear,
    pub to_season: SeasonYear,
    pub seasons_evaluated: Vec<SeasonYear>,
    pub seasons_skipped: Vec<(SeasonYear, String)>,
    pub models: Vec<ModelReport>,
    /// Best in-sample grid point over all seasons (home bonus, scale, mean log loss of the
    /// tuning criterion per trial: per series or per champion); for transparency, not a fair
    /// score.
    pub in_sample_best: Option<(f64, f64, f64)>,
    /// What the tuning minimised, when a grid was given.
    #[serde(default)]
    pub tuning_criterion: Option<TuningCriterion>,
    pub elapsed_ms: u128,
}

fn forecast_rows(input: &SeasonInput, spec: &ModelSpec) -> Vec<TeamForecastRow> {
    let exact = spec
        .exact(&input.bracket)
        .expect("backtest models are stateless");
    input
        .bracket
        .teams
        .iter()
        .zip(exact)
        .map(|(t, probs)| TeamForecastRow {
            season: input.season,
            team_id: t.team_id,
            code: t.code.clone(),
            franchise_key: t.franchise_key.clone(),
            seed: t.seed,
            probs,
            actual: t.actual_result.unwrap_or(PlayoffResult::FirstRound),
            champion: t.actual_result == Some(PlayoffResult::Champion),
        })
        .collect()
}

/// Whether the favourite won, from the winner's probability; `None` when the model gave one
/// half to each side (within rounding).
fn favourite_won(p_winner: f64) -> Option<bool> {
    ((p_winner - 0.5).abs() > 1e-12).then_some(p_winner > 0.5)
}

/// The played series of a season, each scored on the model's probability for its two
/// participants. A model whose ratings move has no exact series probability; the
/// parameters' validation keeps such a model out of a backtest.
fn series_scores(input: &SeasonInput, spec: &ModelSpec) -> Vec<SeriesScore> {
    let b = &input.bracket;
    b.actual_series
        .iter()
        .filter_map(|s| {
            let winner = s.winner_slot?;
            let p_a = spec.series_probability(b, s.slot_a, s.slot_b, s.round)?;
            let (loser, p_winner, games) = if winner == s.slot_a {
                (s.slot_b, p_a, (s.wins_a, s.wins_b))
            } else {
                (s.slot_a, 1.0 - p_a, (s.wins_b, s.wins_a))
            };
            Some(SeriesScore {
                season: input.season,
                round: s.round,
                winner_id: b.teams[winner].team_id,
                loser_id: b.teams[loser].team_id,
                winner: b.teams[winner].code.clone(),
                loser: b.teams[loser].code.clone(),
                games,
                p_winner,
                brier: (p_winner - 1.0).powi(2),
                log_loss: metrics::log_loss(p_winner),
                hit: favourite_won(p_winner),
            })
        })
        .collect()
}

fn score_season(rows: &[TeamForecastRow], series: &[SeriesScore]) -> SeasonScore {
    let n = series.len().max(1) as f64;
    let champ = rows
        .iter()
        .find(|r| r.champion)
        .expect("complete season has a champion");
    let fav = rows
        .iter()
        .max_by(|a, b| a.probs[3].total_cmp(&b.probs[3]))
        .expect("16 teams");
    let probs: Vec<f64> = rows.iter().map(|r| r.probs[3]).collect();
    let outcomes: Vec<bool> = rows.iter().map(|r| r.champion).collect();
    SeasonScore {
        season: champ.season,
        champion: champ.code.clone(),
        favorite: fav.code.clone(),
        favorite_prob: fav.probs[3],
        p_champion: champ.probs[3],
        brier: metrics::multiclass_brier(&probs, &outcomes),
        log_loss: metrics::log_loss(champ.probs[3]),
        hit: fav.team_id == champ.team_id,
        series: series.len(),
        series_brier: series.iter().map(|s| s.brier).sum::<f64>() / n,
        series_log_loss: series.iter().map(|s| s.log_loss).sum::<f64>() / n,
        series_hits: series.iter().filter(|s| s.hit == Some(true)).count(),
        series_with_favourite: series.iter().filter(|s| s.hit.is_some()).count(),
    }
}

fn build_report(
    name: String,
    spec: Option<ModelSpec>,
    forecasts: Vec<TeamForecastRow>,
    series: Vec<SeriesScore>,
    chosen: Option<Vec<(SeasonYear, f64, f64)>>,
) -> ModelReport {
    let mut by_season: BTreeMap<SeasonYear, Vec<&TeamForecastRow>> = BTreeMap::new();
    for r in &forecasts {
        by_season.entry(r.season).or_default().push(r);
    }
    let seasons: Vec<SeasonScore> = by_season
        .iter()
        .map(|(season, rows)| {
            let owned: Vec<TeamForecastRow> = rows.iter().map(|r| (*r).clone()).collect();
            let of_season: Vec<SeriesScore> = series
                .iter()
                .filter(|s| s.season == *season)
                .cloned()
                .collect();
            score_season(&owned, &of_season)
        })
        .collect();
    let summary = metrics::summarise(&seasons);
    let series_summary = metrics::summarise_series(&series);
    let mut calibration = BTreeMap::new();
    let stages: [(&str, usize, PlayoffResult); 4] = [
        ("second_round", 0, PlayoffResult::SecondRound),
        ("conf_finals", 1, PlayoffResult::ConfFinals),
        ("finals", 2, PlayoffResult::Finals),
        ("champion", 3, PlayoffResult::Champion),
    ];
    for (label, idx, at_least) in stages {
        let pairs: Vec<(f64, bool)> = forecasts
            .iter()
            .map(|r| (r.probs[idx], r.actual >= at_least))
            .collect();
        calibration.insert(label.to_string(), metrics::calibration(&pairs));
    }
    let favourites: Vec<(f64, bool)> = series
        .iter()
        .filter_map(|s| s.hit.map(|won| (s.p_winner.max(1.0 - s.p_winner), won)))
        .collect();
    calibration.insert(
        "series".to_string(),
        metrics::calibration_with(&favourites, &metrics::SERIES_EDGES),
    );
    ModelReport {
        name,
        spec,
        seasons,
        summary,
        series_summary,
        series_vs_first: None,
        calibration,
        forecasts,
        series,
        chosen_params: chosen,
    }
}

/// Run the backtest over prepared season inputs (already restricted to complete seasons).
pub fn run(
    params: &BacktestParams,
    inputs: &[SeasonInput],
    skipped: Vec<(SeasonYear, String)>,
) -> BacktestResult {
    let start = std::time::Instant::now();
    let mut models = Vec::new();
    let mut seen_names: BTreeMap<&str, usize> = BTreeMap::new();
    for spec in &params.models {
        let n = seen_names.entry(spec.name()).or_insert(0);
        *n += 1;
        let name = if *n == 1 {
            spec.name().to_string()
        } else {
            format!("{}_{}", spec.name(), n)
        };
        let forecasts: Vec<TeamForecastRow> =
            inputs.iter().flat_map(|i| forecast_rows(i, spec)).collect();
        let series: Vec<SeriesScore> = inputs.iter().flat_map(|i| series_scores(i, spec)).collect();
        models.push(build_report(
            name,
            Some(spec.clone()),
            forecasts,
            series,
            None,
        ));
    }
    let mut in_sample_best = None;
    if let Some(grid) = &params.tuning {
        // The criterion's log loss of every grid point on every season, computed once: the
        // champion's, or the sum over the season's played series.
        let points: Vec<(f64, f64)> = grid
            .home_advantages
            .iter()
            .flat_map(|&h| grid.scales.iter().map(move |&s| (h, s)))
            .collect();
        let losses: Vec<Vec<f64>> = points
            .iter()
            .map(|&(h, s)| {
                let spec = ModelSpec::Elo {
                    home_advantage: h,
                    scale: s,
                    k: None,
                };
                inputs
                    .iter()
                    .map(|i| match grid.criterion {
                        TuningCriterion::Champion => {
                            score_season(&forecast_rows(i, &spec), &[]).log_loss
                        }
                        TuningCriterion::Series => {
                            series_scores(i, &spec).iter().map(|s| s.log_loss).sum()
                        }
                    })
                    .collect()
            })
            .collect();
        // in-sample best (for transparency only), as a mean per trial of the criterion
        let trials = match grid.criterion {
            TuningCriterion::Champion => inputs.len(),
            TuningCriterion::Series => inputs
                .iter()
                .map(|i| {
                    i.bracket
                        .actual_series
                        .iter()
                        .filter(|s| s.winner_slot.is_some())
                        .count()
                })
                .sum(),
        };
        if let Some((pi, _)) = losses.iter().enumerate().min_by(|a, b| {
            let sa: f64 = a.1.iter().sum();
            let sb: f64 = b.1.iter().sum();
            sa.total_cmp(&sb)
        }) {
            let mean = losses[pi].iter().sum::<f64>() / trials.max(1) as f64;
            in_sample_best = Some((points[pi].0, points[pi].1, mean));
        }
        // rolling: for season index t, fit on 0..t
        let mut forecasts = Vec::new();
        let mut series = Vec::new();
        let mut chosen = Vec::new();
        for (t, input) in inputs.iter().enumerate() {
            if t < grid.min_train_seasons {
                continue;
            }
            let best = (0..points.len())
                .min_by(|&a, &b| {
                    let sa: f64 = losses[a][..t].iter().sum();
                    let sb: f64 = losses[b][..t].iter().sum();
                    sa.total_cmp(&sb)
                })
                .expect("non-empty grid");
            let (h, s) = points[best];
            chosen.push((input.season, h, s));
            let spec = ModelSpec::Elo {
                home_advantage: h,
                scale: s,
                k: None,
            };
            forecasts.extend(forecast_rows(input, &spec));
            series.extend(series_scores(input, &spec));
        }
        if !forecasts.is_empty() {
            models.push(build_report(
                "elo_tuned_rolling".into(),
                None,
                forecasts,
                series,
                Some(chosen),
            ));
        }
    }
    // Every model against the first, paired on the series both scored.
    if let Some((first, rest)) = models.split_first_mut() {
        for m in rest {
            m.series_vs_first = metrics::paired_difference(&first.name, &m.series, &first.series);
        }
    }
    BacktestResult {
        model_version: MODEL_VERSION.to_string(),
        from_season: params.from_season,
        to_season: params.to_season,
        seasons_evaluated: inputs.iter().map(|i| i.season).collect(),
        seasons_skipped: skipped,
        models,
        in_sample_best,
        tuning_criterion: params.tuning.as_ref().map(|g| g.criterion),
        elapsed_ms: start.elapsed().as_millis(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::playoffs::bracket::test_bracket;

    /// The winner of the block of `1 << round` slots starting at `start`: the champion when it
    /// is in the block, else the lowest slot; the series played on the way are appended.
    fn play_block(
        start: usize,
        round: u8,
        champ: usize,
        out: &mut Vec<crate::domain::playoffs::bracket::SeriesRecord>,
    ) -> usize {
        if round == 0 {
            return start;
        }
        let half = 1usize << (round - 1);
        let a = play_block(start, round - 1, champ, out);
        let b = play_block(start + half, round - 1, champ, out);
        let winner = if b == champ { b } else { a };
        let (wins_a, wins_b) = if winner == a { (4, 1) } else { (2, 4) };
        out.push(crate::domain::playoffs::bracket::SeriesRecord {
            round: Round::from_number(round).unwrap(),
            slot_a: a,
            slot_b: b,
            winner_slot: Some(winner),
            wins_a,
            wins_b,
            game1_host_slot: a.min(b),
        });
        winner
    }

    fn input(year: i32, champ_slot: usize) -> SeasonInput {
        let season = SeasonYear::of(year);
        let mut elos = [1500.0; 16];
        elos[champ_slot] = 1650.0;
        let mut b = test_bracket(elos);
        b.season = season;
        for (i, t) in b.teams.iter_mut().enumerate() {
            t.actual_result = Some(if i == champ_slot {
                PlayoffResult::Champion
            } else {
                PlayoffResult::FirstRound
            });
        }
        let mut series = Vec::new();
        assert_eq!(play_block(0, 4, champ_slot, &mut series), champ_slot);
        assert_eq!(series.len(), 15);
        // The first series recorded is slots 0 and 1 in the first round, won by slot 0; its
        // game 1 was hosted by slot 1, against the seeding, so only the recorded host makes
        // slot 1 the Elo favourite between two equal ratings.
        assert_eq!((series[0].slot_a, series[0].slot_b), (0, 1));
        series[0].game1_host_slot = 1;
        b.actual_series = series;
        SeasonInput {
            season,
            bracket: b,
            cutoff: NaiveDate::from_ymd_opt(year, 4, 15).expect("valid date"),
        }
    }

    #[test]
    fn elo_beats_equal_when_favourite_wins() {
        let inputs: Vec<SeasonInput> = (2010..2016).map(|s| input(s, (s as usize) % 16)).collect();
        let params = BacktestParams {
            from_season: SeasonYear::of(2010),
            to_season: SeasonYear::of(2015),
            models: default_models(),
            tuning: Some(TuningGrid {
                min_train_seasons: 2,
                ..Default::default()
            }),
        };
        let r = run(&params, &inputs, vec![]);
        let elo = r.models.iter().find(|m| m.name == "elo").unwrap();
        let equal = r.models.iter().find(|m| m.name == "equal").unwrap();
        assert!(elo.summary.mean_log_loss < equal.summary.mean_log_loss);
        assert_eq!(elo.summary.top_pick_accuracy, 1.0);
        assert!((equal.summary.mean_p_winner - 1.0 / 16.0).abs() < 1e-9);
        let tuned = r
            .models
            .iter()
            .find(|m| m.name == "elo_tuned_rolling")
            .unwrap();
        assert_eq!(tuned.seasons.len(), 4);
        // The in-sample best is a mean per series: below a coin flip's ln 2.
        let (_, _, best) = r.in_sample_best.unwrap();
        assert!(best > 0.0 && best < 2f64.ln());
        assert_eq!(r.tuning_criterion, Some(TuningCriterion::Series));
        assert_eq!(elo.forecasts.len(), 16 * 6);
        assert_eq!(tuned.series.len(), 15 * 4);
    }

    #[test]
    fn series_are_scored_one_by_one_with_the_actual_home_court() {
        let inputs: Vec<SeasonInput> = (2010..2013).map(|s| input(s, 11)).collect();
        let params = BacktestParams {
            from_season: SeasonYear::of(2010),
            to_season: SeasonYear::of(2012),
            models: default_models(),
            tuning: None,
        };
        let r = run(&params, &inputs, vec![]);
        let elo = r.models.iter().find(|m| m.name == "elo").unwrap();
        let equal = r.models.iter().find(|m| m.name == "equal").unwrap();
        // Fifteen series a season, each a two-sided trial.
        assert_eq!(elo.series.len(), 45);
        assert_eq!(elo.series_summary.series, 45);
        assert_eq!(elo.seasons[0].series, 15);
        assert_eq!(
            elo.series_summary
                .by_round
                .iter()
                .map(|r| r.series)
                .collect::<Vec<_>>(),
            [24, 12, 6, 3]
        );
        // A coin flip scores exactly 0.25 and ln 2 on every series, and has no favourite.
        assert!((equal.series_summary.mean_brier - 0.25).abs() < 1e-12);
        assert!((equal.series_summary.mean_log_loss - 2f64.ln()).abs() < 1e-12);
        assert_eq!(equal.series_summary.with_favourite, 0);
        assert!(equal.series_summary.brier_skill_vs_equal.abs() < 1e-12);
        assert!(equal.series.iter().all(|s| s.hit.is_none()));
        // Elo favours the champion in its four series and the game-1 host everywhere else;
        // the host won every series but the one per season whose recorded host was the higher slot,
        // where the recorded home court, not the seeding, names the favourite.
        assert_eq!(elo.series_summary.with_favourite, 45);
        assert!((elo.series_summary.favourite_won - 42.0 / 45.0).abs() < 1e-12);
        let upset = elo
            .series
            .iter()
            .find(|s| s.season == SeasonYear::of(2010) && s.winner == "T0")
            .unwrap();
        assert_eq!(
            (upset.round, upset.loser.as_str(), upset.hit),
            (Round::First, "T1", Some(false))
        );
        assert!(upset.p_winner < 0.5);
        assert!(elo.series_summary.mean_p_favourite > 0.5);
        assert!(elo.series_summary.mean_log_loss < equal.series_summary.mean_log_loss);
        assert!(elo.series_summary.brier_skill_vs_equal > 0.0);
        let finals = elo
            .series
            .iter()
            .find(|s| s.round == Round::Finals)
            .unwrap();
        assert_eq!((finals.winner.as_str(), finals.games), ("T11", (4, 2)));
        assert!(finals.p_winner > 0.5 && finals.brier < 0.25);
        // The season rows carry the same series totals.
        let season = &elo.seasons[0];
        assert_eq!((season.series_hits, season.series_with_favourite), (14, 15));
        assert_eq!(equal.seasons[0].series_with_favourite, 0);
        assert!((season.series_log_loss - elo.series_summary.mean_log_loss).abs() < 1e-12);
        // Paired against the first model: equal minus elo is positive on every series.
        let paired = equal.series_vs_first.as_ref().unwrap();
        assert_eq!((paired.against.as_str(), paired.series), ("elo", 45));
        assert!(paired.log_loss > 0.0 && paired.brier > 0.0);
        assert!(elo.series_vs_first.is_none());
        // Without a grid there is no criterion; a K-factor model is refused.
        assert_eq!(r.tuning_criterion, None);
        let stateful = BacktestParams {
            models: vec![ModelSpec::Elo {
                home_advantage: 100.0,
                scale: 1.0,
                k: Some(20.0),
            }],
            ..params.clone()
        };
        assert!(stateful.validate().is_err());
        // The favourite's calibration bins hold every series with a favourite.
        assert_eq!(
            elo.calibration["series"]
                .iter()
                .map(|b| b.count)
                .sum::<usize>(),
            45
        );
    }
}
