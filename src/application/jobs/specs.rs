//! Concrete job types. Each wraps an engine's parameter struct and knows how to load its
//! inputs from the application and run the engine on the blocking pool.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::application::dynasty::{self, DynastyRequest};
use crate::application::ports::{AnalysisStore, NewForecast};
use crate::domain::backtesting::{self, BacktestParams, BacktestResult, SeasonInput};
use crate::domain::ids::SeasonYear;
use crate::domain::models::{GameType, JobKind, PlayoffResult};
use crate::domain::playoffs::bracket::{Bracket, BracketSimulation, ModelSpec};
use crate::domain::playoffs::season::{self, SeasonForecast, SeasonParams};
use crate::domain::simulation::alpha::{self, AlphaParams, AlphaResult};
use crate::domain::simulation::dynasty::DynastyReport;
use crate::domain::simulation::progress::Progress;
use crate::domain::validation::{invalid, ValidationError};

use super::{JobContext, JobError, JobSpec, Limits};

// ---------------------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AlphaJob(pub AlphaParams);

impl JobSpec for AlphaJob {
    const KIND: JobKind = JobKind::Alpha;
    const MODEL_VERSION: &'static str = alpha::MODEL_VERSION;
    type Output = AlphaResult;

    fn validate(&self, limits: &Limits) -> Result<(), ValidationError> {
        self.0.validate(limits.max_iterations)?;
        // Each iteration draws one winner per season and updates one counter per franchise.
        limits.check_work(
            self.0.iterations,
            (self.0.seasons + self.0.franchises.len()) as u64,
        )
    }
    fn seed(&self) -> Option<i64> {
        Some(self.0.seed as i64)
    }
    async fn execute(self, ctx: JobContext) -> Result<AlphaResult, JobError> {
        let progress = ctx.progress;
        tokio::task::spawn_blocking(move || alpha::run(&self.0, &progress))
            .await
            .map_err(|e| JobError::Execution(e.to_string()))
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
    pub season: SeasonYear,
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

    fn validate(&self, limits: &Limits) -> Result<(), ValidationError> {
        if self.iterations == 0 || self.iterations > limits.max_iterations {
            return invalid(
                "iterations",
                format!("iterations must be between 1 and {}", limits.max_iterations),
            );
        }
        self.model.validate()?;
        // At most 15 series of 7 games per iteration.
        limits.check_work(self.iterations, 105)?;
        if self.elo_overrides.len() > 16
            || self
                .elo_overrides
                .values()
                .any(|e| !(800.0..=2500.0).contains(e))
        {
            return invalid(
                "elo_overrides",
                "at most 16 overrides, each between 800 and 2500",
            );
        }
        Ok(())
    }
    fn seed(&self) -> Option<i64> {
        Some(self.seed as i64)
    }
    async fn execute(self, ctx: JobContext) -> Result<PlayoffOutput, JobError> {
        let data = ctx
            .app
            .season_data(self.season)
            .await
            .map_err(|e| JobError::Execution(e.to_string()))?;
        let mut bracket = data
            .reconstruction
            .as_ref()
            .map_err(|e| JobError::Execution(e.to_string()))?
            .bracket
            .clone();
        if self.elo_source == EloSource::RegularSeasonEnd {
            let regular_end: HashMap<_, f64> = data
                .games
                .iter()
                .filter(|g| g.game_type == GameType::Regular && g.is_played())
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
        let actual_champion = bracket
            .teams
            .iter()
            .find(|t| t.actual_result == Some(PlayoffResult::Champion))
            .map(|t| t.code.clone());
        let information_cutoff = data.pre_playoff_cutoff();
        let progress = ctx.progress;
        let model = self.model.clone();
        let b2 = bracket.clone();
        let simulation = tokio::task::spawn_blocking(move || {
            model.simulate(&b2, self.iterations, self.seed, &progress)
        })
        .await
        .map_err(|e| JobError::Execution(e.to_string()))?;
        Ok(PlayoffOutput {
            bracket,
            simulation,
            model: self.model,
            elo_source: self.elo_source,
            information_cutoff,
            actual_champion,
        })
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

    fn validate(&self, limits: &Limits) -> Result<(), ValidationError> {
        self.0.validate(limits.max_iterations)?;
        // A full regular season (1,230 games), the play-in and the playoffs, plus sorting;
        // twice with the held run.
        limits.check_work(
            self.0.iterations,
            if self.0.hold_absences { 3_000 } else { 1_500 },
        )
    }
    fn seed(&self) -> Option<i64> {
        Some(self.0.seed as i64)
    }
    async fn execute(self, ctx: JobContext) -> Result<SeasonForecast, JobError> {
        let data = ctx
            .app
            .season_data(self.0.season)
            .await
            .map_err(|e| JobError::Execution(e.to_string()))?;
        let teams = ctx.app.teams().await?;
        let progress = ctx.progress;
        let params = self.0;
        tokio::task::spawn_blocking(move || season::run(&params, &data.games, &teams, &progress))
            .await
            .map_err(|e| JobError::Execution(e.to_string()))?
            .map_err(|e| JobError::Execution(e.to_string()))
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

    fn validate(&self, _: &Limits) -> Result<(), ValidationError> {
        self.0.validate()
    }
    async fn execute(self, ctx: JobContext) -> Result<BacktestResult, JobError> {
        let params = self.0;
        let complete = ctx
            .app
            .store
            .complete_playoff_seasons(params.from_season, params.to_season)
            .await?;
        let mut inputs = Vec::new();
        let mut skipped = Vec::new();
        let total = params.to_season.seasons_since(params.from_season).max(1) as f64;
        for (i, season) in SeasonYear::range(params.from_season, params.to_season).enumerate() {
            ctx.progress.report(0.5 * i as f64 / total);
            if !complete.contains(&season) {
                skipped.push((
                    season,
                    "no complete playoff record in the imported data".to_string(),
                ));
                continue;
            }
            let data = match ctx.app.season_data(season).await {
                Ok(d) => d,
                Err(e) => {
                    skipped.push((season, e.to_string()));
                    continue;
                }
            };
            match (&data.reconstruction, data.pre_playoff_cutoff()) {
                (Ok(r), Some(cutoff)) if r.complete => inputs.push(SeasonInput {
                    season,
                    bracket: r.bracket.clone(),
                    cutoff,
                }),
                (Ok(_), _) => skipped.push((season, "postseason incomplete".into())),
                (Err(e), _) => skipped.push((season, e.to_string())),
            }
        }
        let cutoffs: HashMap<SeasonYear, chrono::NaiveDate> =
            inputs.iter().map(|i| (i.season, i.cutoff)).collect();
        let result =
            tokio::task::spawn_blocking(move || backtesting::run(&params, &inputs, skipped))
                .await
                .map_err(|e| JobError::Execution(e.to_string()))?;
        ctx.progress.report(0.9);
        // Persist per-team forecasts for the validation page.
        for model in &result.models {
            for f in &model.forecasts {
                let Some(&cutoff) = cutoffs.get(&f.season) else {
                    continue;
                };
                let row = NewForecast {
                    job_id: ctx.job_id,
                    model: &model.name,
                    season: f.season,
                    team: f.team_id,
                    seed: Some(i32::from(f.seed)),
                    probs: f.probs,
                    actual_result: f.actual,
                    actual_champion: f.champion,
                    cutoff,
                };
                ctx.app.store.insert_forecast(&row).await?;
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

    fn validate(&self, _: &Limits) -> Result<(), ValidationError> {
        self.0.validate()
    }
    async fn execute(self, ctx: JobContext) -> Result<DynastyReport, JobError> {
        dynasty::analyze(&ctx.app, &self.0)
            .await
            .map_err(|e| JobError::Execution(e.to_string()))
    }
}
