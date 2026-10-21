//! Concrete job types. Each wraps an engine's parameter struct and knows how to load its
//! inputs from the application and run the engine on the blocking pool.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::domain::backtesting::{self, BacktestParams, BacktestResult, SeasonInput};
use crate::adapters::postgres as db;
use crate::domain::models::{JobKind, PlayoffResult};
use crate::domain::playoffs::bracket::{Bracket, BracketSimulation, ModelSpec};
use crate::domain::playoffs::season::{self, SeasonForecast, SeasonParams};
use crate::application::dynasty::{self, DynastyRequest};
use crate::domain::simulation::alpha::{self, AlphaParams, AlphaResult};
use crate::domain::simulation::dynasty::DynastyReport;
use crate::domain::simulation::progress::Progress;

use super::{JobContext, JobSpec, Limits};

// ---------------------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AlphaJob(pub AlphaParams);

impl JobSpec for AlphaJob {
    const KIND: JobKind = JobKind::Alpha;
    const MODEL_VERSION: &'static str = alpha::MODEL_VERSION;
    type Output = AlphaResult;

    fn validate(&self, limits: &Limits) -> Result<(), String> {
        self.0.validate(limits.max_iterations)
    }
    fn seed(&self) -> Option<i64> {
        Some(self.0.seed as i64)
    }
    async fn execute(self, ctx: JobContext) -> anyhow::Result<AlphaResult> {
        let progress = ctx.progress;
        Ok(tokio::task::spawn_blocking(move || alpha::run(&self.0, &progress)).await?)
    }
}

// ---------------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum EloSource {
    /// Rating after the last game before the playoffs (includes play-in games).
    #[default]
    PrePlayoffs,
    /// Rating after the last regular-season game.
    RegularSeasonEnd,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayoffJob {
    pub season: i32,
    pub iterations: u64,
    pub seed: u64,
    #[serde(default)]
    pub model: ModelSpec,
    #[serde(default)]
    pub elo_source: EloSource,
    /// Optional manual rating overrides by team code.
    #[serde(default)]
    pub elo_overrides: HashMap<String, f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayoffOutput {
    pub bracket: Bracket,
    pub simulation: BracketSimulation,
    pub model: ModelSpec,
    pub elo_source: EloSource,
    pub information_cutoff: Option<chrono::NaiveDate>,
    pub actual_champion: Option<String>,
}

impl JobSpec for PlayoffJob {
    const KIND: JobKind = JobKind::Playoff;
    const MODEL_VERSION: &'static str = crate::domain::playoffs::MODEL_VERSION;
    type Output = PlayoffOutput;

    fn validate(&self, limits: &Limits) -> Result<(), String> {
        if self.iterations == 0 || self.iterations > limits.max_iterations {
            return Err(format!("iterations must be between 1 and {}", limits.max_iterations));
        }
        if !(1947..=2100).contains(&self.season) {
            return Err("season out of range".into());
        }
        self.model.validate()?;
        if self.elo_overrides.len() > 16 || self.elo_overrides.values().any(|e| !(800.0..=2500.0).contains(e)) {
            return Err("Elo overrides must be between 800 and 2500".into());
        }
        Ok(())
    }
    fn seed(&self) -> Option<i64> {
        Some(self.seed as i64)
    }
    async fn execute(self, ctx: JobContext) -> anyhow::Result<PlayoffOutput> {
        let data = ctx.app.season_data(self.season).await?;
        let mut bracket = data.reconstruction.as_ref().map_err(|e| anyhow::anyhow!("{e}"))?.bracket.clone();
        if self.elo_source == EloSource::RegularSeasonEnd {
            let regular_end: HashMap<i32, f64> = data
                .games
                .iter()
                .filter(|g| g.game_type == crate::domain::models::GameType::Regular && g.is_played())
                .fold(HashMap::new(), |mut m, g| {
                    m.insert(g.home_team_id, g.home_elo_post);
                    m.insert(g.away_team_id, g.away_elo_post);
                    m
                });
            for t in bracket.teams.iter_mut() {
                if let Some(e) = regular_end.get(&t.team_id) {
                    t.elo = *e;
                }
            }
        }
        for t in bracket.teams.iter_mut() {
            if let Some(e) = self.elo_overrides.get(&t.code) {
                t.elo = *e;
            }
        }
        let actual_champion = bracket.teams.iter().find(|t| t.actual_result == Some(PlayoffResult::Champion)).map(|t| t.code.clone());
        let information_cutoff = data.pre_playoff_cutoff();
        let progress = ctx.progress;
        let model = self.model.clone();
        let b2 = bracket.clone();
        let simulation = tokio::task::spawn_blocking(move || model.simulate(&b2, self.iterations, self.seed, &progress)).await?;
        Ok(PlayoffOutput { bracket, simulation, model: self.model, elo_source: self.elo_source, information_cutoff, actual_champion })
    }
}

// ---------------------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SeasonJob(pub SeasonParams);

impl JobSpec for SeasonJob {
    const KIND: JobKind = JobKind::Season;
    const MODEL_VERSION: &'static str = crate::domain::playoffs::SEASON_MODEL_VERSION;
    type Output = SeasonForecast;

    fn validate(&self, limits: &Limits) -> Result<(), String> {
        self.0.validate(limits.max_iterations)
    }
    fn seed(&self) -> Option<i64> {
        Some(self.0.seed as i64)
    }
    async fn execute(self, ctx: JobContext) -> anyhow::Result<SeasonForecast> {
        let data = ctx.app.season_data(self.0.season).await?;
        let teams = ctx.app.teams().await?;
        let progress = ctx.progress;
        let params = self.0;
        Ok(tokio::task::spawn_blocking(move || season::run(&params, &data.games, &teams, &progress)).await??)
    }
}

// ---------------------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(transparent)]
pub struct BacktestJob(pub BacktestParams);

impl JobSpec for BacktestJob {
    const KIND: JobKind = JobKind::Backtest;
    const MODEL_VERSION: &'static str = backtesting::MODEL_VERSION;
    type Output = BacktestResult;

    fn validate(&self, _: &Limits) -> Result<(), String> {
        self.0.validate()
    }
    async fn execute(self, ctx: JobContext) -> anyhow::Result<BacktestResult> {
        let params = self.0;
        let seasons = db::analysis::complete_playoff_seasons(&ctx.app.pool, params.from_season, params.to_season).await?;
        let mut inputs = Vec::new();
        let mut skipped = Vec::new();
        let total = (params.to_season - params.from_season + 1) as f64;
        for s in params.from_season..=params.to_season {
            ctx.progress.report(0.5 * (s - params.from_season) as f64 / total);
            if !seasons.contains(&s) {
                skipped.push((s, "no complete playoff record in the imported data".to_string()));
                continue;
            }
            let data = match ctx.app.season_data(s).await {
                Ok(d) => d,
                Err(e) => {
                    skipped.push((s, e.to_string()));
                    continue;
                }
            };
            match (&data.reconstruction, data.pre_playoff_cutoff()) {
                (Ok(r), Some(cutoff)) if r.complete => inputs.push(SeasonInput { season: s, bracket: r.bracket.clone(), cutoff }),
                (Ok(_), _) => skipped.push((s, "postseason incomplete".into())),
                (Err(e), _) => skipped.push((s, e.to_string())),
            }
        }
        let result = tokio::task::spawn_blocking(move || backtesting::run(&params, &inputs, skipped)).await?;
        ctx.progress.report(0.9);
        // Persist per-team forecasts for the validation page.
        for model in &result.models {
            for f in &model.forecasts {
                let cutoff = match ctx.app.season_data(f.season).await.ok().and_then(|d| d.pre_playoff_cutoff()) {
                    Some(c) => c,
                    None => chrono::NaiveDate::from_ymd_opt(f.season, 4, 1).expect("valid date"),
                };
                let row = db::analysis::NewForecast {
                    job_id: ctx.job_id,
                    model: &model.name,
                    season: f.season,
                    team_id: f.team_id,
                    seed: Some(i32::from(f.seed)),
                    probs: f.probs,
                    actual_result: f.actual,
                    actual_champion: f.champion,
                    cutoff,
                };
                db::analysis::insert_forecast(&ctx.app.pool, &row).await?;
            }
        }
        ctx.progress.report(1.0);
        Ok(result)
    }
}

// ---------------------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(transparent)]
pub struct DynastyJob(pub DynastyRequest);

impl JobSpec for DynastyJob {
    const KIND: JobKind = JobKind::Dynasty;
    const MODEL_VERSION: &'static str = crate::domain::simulation::dynasty::MODEL_VERSION;
    type Output = DynastyReport;

    fn validate(&self, _: &Limits) -> Result<(), String> {
        self.0.validate()
    }
    async fn execute(self, ctx: JobContext) -> anyhow::Result<DynastyReport> {
        dynasty::analyze(&ctx.app, &self.0).await
    }
}
