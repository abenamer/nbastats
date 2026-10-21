//! Assemble the dynasty report for a franchise and season range from stored data.

use serde::{Deserialize, Serialize};

use crate::app::App;
use crate::db;
use crate::models::PlayoffResult;
use crate::playoffs::bracket::ModelSpec;
use crate::simulation::alpha::{self, AlphaParams};
use crate::simulation::dynasty::{compare, DynastyReport, DynastySeason, MODEL_VERSION};
use crate::simulation::exact;
use crate::simulation::progress::Silent;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DynastyRequest {
    pub franchise: String,
    pub from_season: i32,
    pub to_season: i32,
    /// Persistent alpha for the selected franchise (others have alpha 1).
    #[serde(default = "default_alpha")]
    pub alpha: f64,
    #[serde(default)]
    pub model: ModelSpec,
}

fn default_alpha() -> f64 {
    3.0
}

impl DynastyRequest {
    pub fn validate(&self) -> Result<(), String> {
        if self.from_season < 1947 || self.to_season < self.from_season || self.to_season - self.from_season > 80 {
            return Err("invalid season range".into());
        }
        if !(0.0..=1000.0).contains(&self.alpha) {
            return Err("alpha must be between 0 and 1000".into());
        }
        if self.franchise.len() > 8 || !self.franchise.chars().all(|c| c.is_ascii_alphanumeric()) {
            return Err("invalid franchise key".into());
        }
        self.model.validate()
    }
}

/// League-wide concentration comparison for the same period.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConcentrationComparison {
    pub threshold: usize,
    pub seasons: usize,
    pub franchises: usize,
    pub p_any_equal: f64,
    pub p_any_alpha: f64,
    pub p_any_elo: f64,
    pub p_any_elo_se: f64,
    pub max_distribution_equal: Vec<f64>,
    pub max_distribution_alpha: Vec<f64>,
    pub max_distribution_elo: Vec<f64>,
    pub actual_max_titles: usize,
    pub elo_seasons_used: Vec<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DynastyResponse {
    #[serde(flatten)]
    pub report: DynastyReport,
    pub concentration: Option<ConcentrationComparison>,
}

pub async fn analyze(app: &App, req: &DynastyRequest) -> anyhow::Result<DynastyReport> {
    let franchise = db::teams::franchise_by_key(&app.pool, &req.franchise)
        .await?
        .ok_or_else(|| anyhow::anyhow!("unknown franchise {}", req.franchise))?;
    let rows = db::analysis::franchise_seasons(&app.pool, franchise.id, req.from_season, req.to_season).await?;
    let mut seasons = Vec::new();
    for ts in rows {
        let data = app.season_data(ts.season).await?;
        let team_count = {
            let mut ids: Vec<i32> = data.games.iter().flat_map(|g| [g.home_team_id, g.away_team_id]).collect();
            ids.sort_unstable();
            ids.dedup();
            ids.len() as i32
        };
        let p_elo = match &data.reconstruction {
            Ok(r) if r.complete || data.pre_playoff_cutoff().is_some() => r
                .bracket
                .teams
                .iter()
                .position(|t| t.team_id == ts.team_id)
                .and_then(|slot| req.model.exact(&r.bracket).map(|e| e[slot][3]))
                .unwrap_or(0.0),
            _ => 0.0,
        };
        let n = f64::from(team_count.max(2));
        seasons.push(DynastySeason {
            season: ts.season,
            code: ts.code,
            wins: ts.wins,
            losses: ts.losses,
            elo_regular_end: ts.elo_regular_end,
            elo_pre_playoffs: ts.elo_pre_playoffs,
            made_playoffs: ts.made_playoffs,
            seed: ts.seed,
            actual_result: ts.playoff_result,
            champion: ts.playoff_result == PlayoffResult::Champion,
            p_elo,
            p_lottery: 1.0 / n,
            p_alpha: req.alpha / (req.alpha + n - 1.0),
            team_count,
        });
    }
    let actual_titles = seasons.iter().filter(|s| s.champion).count();
    let comparisons = compare(&seasons, actual_titles);
    Ok(DynastyReport {
        model_version: MODEL_VERSION.to_string(),
        franchise_key: franchise.key,
        franchise_name: franchise.name,
        from_season: req.from_season,
        to_season: req.to_season,
        alpha: req.alpha,
        seasons,
        actual_titles,
        comparisons,
    })
}

/// Compare league-wide title concentration under equal strength, a persistent alpha for the
/// selected franchise, and season-varying Elo championship probabilities (Engine A in
/// schedule mode, fed with Engine B's exact per-season probabilities).
pub async fn concentration(app: &App, req: &DynastyRequest, threshold: usize) -> anyhow::Result<Option<ConcentrationComparison>> {
    let franchises = db::teams::active_franchises(&app.pool).await?;
    let keys: Vec<String> = franchises.iter().map(|f| f.key.clone()).collect();
    let n = keys.len();
    let mut schedule: Vec<Vec<f64>> = Vec::new();
    let mut used = Vec::new();
    for season in req.from_season..=req.to_season {
        let Ok(data) = app.season_data(season).await else { continue };
        let Ok(r) = &data.reconstruction else { continue };
        if !r.complete {
            continue;
        }
        let Some(exact) = req.model.exact(&r.bracket) else { continue };
        let mut row = vec![0.0; n];
        for (slot, t) in r.bracket.teams.iter().enumerate() {
            if let Some(i) = keys.iter().position(|k| *k == t.franchise_key) {
                row[i] += exact[slot][3];
            }
        }
        if row.iter().sum::<f64>() > 0.0 {
            schedule.push(row);
            used.push(season);
        }
    }
    if schedule.is_empty() {
        return Ok(None);
    }
    let seasons = schedule.len();
    let threshold = threshold.clamp(1, seasons);
    let titles = db::analysis::title_counts(&app.pool, req.from_season, req.to_season).await?;
    let actual_max = titles.values().copied().max().unwrap_or(0) as usize;
    let equal = vec![1.0; n];
    let mut alpha_vec = vec![1.0; n];
    if let Some(i) = keys.iter().position(|k| *k == req.franchise) {
        alpha_vec[i] = req.alpha;
    }
    let mut params = AlphaParams::equal(n, seasons, 50_000, threshold, 20240601);
    params.franchises = keys.clone();
    params.schedule = Some(schedule);
    let elo_result = tokio::task::spawn_blocking(move || alpha::run(&params, &Silent)).await?;
    Ok(Some(ConcentrationComparison {
        threshold,
        seasons,
        franchises: n,
        p_any_equal: exact::p_any_at_least(&equal, seasons, threshold),
        p_any_alpha: exact::p_any_at_least(&alpha_vec, seasons, threshold),
        p_any_elo: elo_result.p_any_at_least_k,
        p_any_elo_se: elo_result.p_any_at_least_k_se,
        max_distribution_equal: exact::max_titles_distribution(&equal, seasons),
        max_distribution_alpha: exact::max_titles_distribution(&alpha_vec, seasons),
        max_distribution_elo: elo_result.max_titles_distribution,
        actual_max_titles: actual_max,
        elo_seasons_used: used,
    }))
}
