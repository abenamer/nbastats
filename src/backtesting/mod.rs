//! Historical validation: leakage-free pre-playoff forecasts, scoring rules, calibration and
//! baselines, with chronological rolling tuning of the Elo parameters.

pub mod metrics;

use std::collections::BTreeMap;

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

use crate::models::PlayoffResult;
use crate::playoffs::bracket::{Bracket, ModelSpec};

pub const MODEL_VERSION: &str = "backtest-1.0";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TuningGrid {
    pub home_advantages: Vec<f64>,
    pub scales: Vec<f64>,
    /// Minimum number of earlier seasons before a season is evaluated with tuned parameters.
    pub min_train_seasons: usize,
}

impl Default for TuningGrid {
    fn default() -> Self {
        Self { home_advantages: vec![0.0, 50.0, 100.0, 150.0], scales: vec![0.75, 1.0, 1.25, 1.5, 1.75], min_train_seasons: 5 }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BacktestParams {
    pub from_season: i32,
    pub to_season: i32,
    #[serde(default = "default_models")]
    pub models: Vec<ModelSpec>,
    #[serde(default = "default_tuning")]
    pub tuning: Option<TuningGrid>,
}

fn default_models() -> Vec<ModelSpec> {
    vec![ModelSpec::default(), ModelSpec::Equal, ModelSpec::WinPct, ModelSpec::Seed { p_better: 0.75 }]
}
fn default_tuning() -> Option<TuningGrid> {
    Some(TuningGrid::default())
}

impl BacktestParams {
    pub fn validate(&self) -> Result<(), String> {
        if self.from_season < 1984 || self.to_season < self.from_season || self.to_season - self.from_season > 100 {
            return Err("season range must start in 1984 or later and be at most 100 seasons".into());
        }
        if self.models.is_empty() || self.models.len() > 12 {
            return Err("between 1 and 12 models are allowed".into());
        }
        for m in &self.models {
            m.validate()?;
        }
        if let Some(t) = &self.tuning {
            if t.home_advantages.is_empty() || t.scales.is_empty() || t.home_advantages.len() * t.scales.len() > 400 {
                return Err("tuning grid must have between 1 and 400 points".into());
            }
        }
        Ok(())
    }
}

/// One season's bracket together with the information cutoff used to build it.
pub struct SeasonInput {
    pub season: i32,
    pub bracket: Bracket,
    pub cutoff: NaiveDate,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamForecastRow {
    pub season: i32,
    pub team_id: i32,
    pub code: String,
    pub franchise_key: String,
    pub seed: u8,
    pub probs: [f64; 4],
    pub actual: PlayoffResult,
    pub champion: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SeasonScore {
    pub season: i32,
    pub champion: String,
    pub favorite: String,
    pub favorite_prob: f64,
    pub p_champion: f64,
    pub brier: f64,
    pub log_loss: f64,
    pub hit: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelReport {
    pub name: String,
    pub spec: Option<ModelSpec>,
    pub seasons: Vec<SeasonScore>,
    pub summary: metrics::Summary,
    pub calibration: BTreeMap<String, Vec<metrics::CalibrationBin>>,
    pub forecasts: Vec<TeamForecastRow>,
    /// For the rolling-tuned model: the parameters chosen for each evaluated season.
    pub chosen_params: Option<Vec<(i32, f64, f64)>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BacktestResult {
    pub model_version: String,
    pub from_season: i32,
    pub to_season: i32,
    pub seasons_evaluated: Vec<i32>,
    pub seasons_skipped: Vec<(i32, String)>,
    pub models: Vec<ModelReport>,
    /// Best in-sample grid point over all seasons (reported for transparency; not a fair score).
    pub in_sample_best: Option<(f64, f64, f64)>,
    pub elapsed_ms: u128,
}

fn forecast_rows(input: &SeasonInput, spec: &ModelSpec) -> Vec<TeamForecastRow> {
    let exact = spec.exact(&input.bracket).expect("backtest models are stateless");
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

fn score_season(rows: &[TeamForecastRow]) -> SeasonScore {
    let champ = rows.iter().find(|r| r.champion).expect("complete season has a champion");
    let fav = rows.iter().max_by(|a, b| a.probs[3].partial_cmp(&b.probs[3]).unwrap()).expect("16 teams");
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
    }
}

fn build_report(name: String, spec: Option<ModelSpec>, forecasts: Vec<TeamForecastRow>, chosen: Option<Vec<(i32, f64, f64)>>) -> ModelReport {
    let mut by_season: BTreeMap<i32, Vec<&TeamForecastRow>> = BTreeMap::new();
    for r in &forecasts {
        by_season.entry(r.season).or_default().push(r);
    }
    let seasons: Vec<SeasonScore> = by_season
        .values()
        .map(|rows| {
            let owned: Vec<TeamForecastRow> = rows.iter().map(|r| (*r).clone()).collect();
            score_season(&owned)
        })
        .collect();
    let summary = metrics::summarise(&seasons);
    let mut calibration = BTreeMap::new();
    let stages: [(&str, usize, PlayoffResult); 4] = [
        ("second_round", 0, PlayoffResult::SecondRound),
        ("conf_finals", 1, PlayoffResult::ConfFinals),
        ("finals", 2, PlayoffResult::Finals),
        ("champion", 3, PlayoffResult::Champion),
    ];
    for (label, idx, at_least) in stages {
        let pairs: Vec<(f64, bool)> = forecasts.iter().map(|r| (r.probs[idx], r.actual >= at_least)).collect();
        calibration.insert(label.to_string(), metrics::calibration(&pairs));
    }
    ModelReport { name, spec, seasons, summary, calibration, forecasts, chosen_params: chosen }
}

/// Run the backtest over prepared season inputs (already restricted to complete seasons).
pub fn run(params: &BacktestParams, inputs: &[SeasonInput], skipped: Vec<(i32, String)>) -> BacktestResult {
    let start = std::time::Instant::now();
    let mut models = Vec::new();
    let mut seen_names: BTreeMap<&str, usize> = BTreeMap::new();
    for spec in &params.models {
        let n = seen_names.entry(spec.name()).or_insert(0);
        *n += 1;
        let name = if *n == 1 { spec.name().to_string() } else { format!("{}_{}", spec.name(), n) };
        let forecasts: Vec<TeamForecastRow> = inputs.iter().flat_map(|i| forecast_rows(i, spec)).collect();
        models.push(build_report(name, Some(spec.clone()), forecasts, None));
    }
    let mut in_sample_best = None;
    if let Some(grid) = &params.tuning {
        // Log loss of every grid point on every season, computed once.
        let points: Vec<(f64, f64)> = grid.home_advantages.iter().flat_map(|&h| grid.scales.iter().map(move |&s| (h, s))).collect();
        let losses: Vec<Vec<f64>> = points
            .iter()
            .map(|&(h, s)| {
                let spec = ModelSpec::Elo { home_advantage: h, scale: s, k: None };
                inputs.iter().map(|i| score_season(&forecast_rows(i, &spec)).log_loss).collect()
            })
            .collect();
        // in-sample best (for transparency only)
        if let Some((pi, _)) = losses.iter().enumerate().min_by(|a, b| {
            let sa: f64 = a.1.iter().sum();
            let sb: f64 = b.1.iter().sum();
            sa.partial_cmp(&sb).unwrap()
        }) {
            let mean = losses[pi].iter().sum::<f64>() / inputs.len().max(1) as f64;
            in_sample_best = Some((points[pi].0, points[pi].1, mean));
        }
        // rolling: for season index t, fit on 0..t
        let mut forecasts = Vec::new();
        let mut chosen = Vec::new();
        for (t, input) in inputs.iter().enumerate() {
            if t < grid.min_train_seasons {
                continue;
            }
            let best = (0..points.len())
                .min_by(|&a, &b| {
                    let sa: f64 = losses[a][..t].iter().sum();
                    let sb: f64 = losses[b][..t].iter().sum();
                    sa.partial_cmp(&sb).unwrap()
                })
                .expect("non-empty grid");
            let (h, s) = points[best];
            chosen.push((input.season, h, s));
            forecasts.extend(forecast_rows(input, &ModelSpec::Elo { home_advantage: h, scale: s, k: None }));
        }
        if !forecasts.is_empty() {
            models.push(build_report("elo_tuned_rolling".into(), None, forecasts, Some(chosen)));
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
        elapsed_ms: start.elapsed().as_millis(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::playoffs::bracket::test_bracket;

    fn input(season: i32, champ_slot: usize) -> SeasonInput {
        let mut elos = [1500.0; 16];
        elos[champ_slot] = 1650.0;
        let mut b = test_bracket(elos);
        b.season = season;
        for (i, t) in b.teams.iter_mut().enumerate() {
            t.actual_result = Some(if i == champ_slot { PlayoffResult::Champion } else { PlayoffResult::FirstRound });
        }
        SeasonInput { season, bracket: b, cutoff: NaiveDate::from_ymd_opt(season, 4, 15).unwrap() }
    }

    #[test]
    fn elo_beats_equal_when_favourite_wins() {
        let inputs: Vec<SeasonInput> = (2010..2016).map(|s| input(s, (s as usize) % 16)).collect();
        let params = BacktestParams { from_season: 2010, to_season: 2015, models: default_models(), tuning: Some(TuningGrid { min_train_seasons: 2, ..Default::default() }) };
        let r = run(&params, &inputs, vec![]);
        let elo = r.models.iter().find(|m| m.name == "elo").unwrap();
        let equal = r.models.iter().find(|m| m.name == "equal").unwrap();
        assert!(elo.summary.mean_log_loss < equal.summary.mean_log_loss);
        assert_eq!(elo.summary.top_pick_accuracy, 1.0);
        assert!((equal.summary.mean_p_winner - 1.0 / 16.0).abs() < 1e-9);
        let tuned = r.models.iter().find(|m| m.name == "elo_tuned_rolling").unwrap();
        assert_eq!(tuned.seasons.len(), 4);
        assert!(r.in_sample_best.is_some());
        assert_eq!(elo.forecasts.len(), 16 * 6);
    }
}
