//! Use case: dynasty report for a franchise and season range.

use serde::{Deserialize, Serialize};

use crate::domain::ids::SeasonYear;
use crate::domain::models::PlayoffResult;
use crate::domain::playoffs::bracket::ModelSpec;
use crate::domain::records::{League, TeamSeasonStat};
use crate::domain::simulation::alpha::{self, AlphaParams};
use crate::domain::simulation::dynasty::{
    compare, strength_seasons, strength_summary, DynastyReport, DynastySeason, FieldRow,
    MODEL_VERSION,
};
use crate::domain::simulation::exact;
use crate::domain::simulation::progress::Silent;
use crate::domain::validation::{invalid, ValidationError};

use super::ports::{AnalysisStore, GameLog, StoreError};
use super::state::{App, SeasonLoadError};
use super::strength::{self, Answer, Record, StrengthError};
use tracing::error;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DynastyRequest {
    pub franchise: String,
    pub from_season: SeasonYear,
    pub to_season: SeasonYear,
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
    pub fn validate(&self) -> Result<(), ValidationError> {
        if self.to_season < self.from_season {
            return invalid("from_season", "the season range ends before it starts");
        }
        if !(0.0..=1000.0).contains(&self.alpha) {
            return invalid("alpha", "alpha must be between 0 and 1000");
        }
        if self.franchise.len() > 8 || !self.franchise.chars().all(|c| c.is_ascii_alphanumeric()) {
            return invalid("franchise", "invalid franchise key");
        }
        self.model.validate()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum DynastyError {
    #[error("unknown franchise {0}")]
    UnknownFranchise(String),
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error(transparent)]
    Season(#[from] SeasonLoadError),
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
    pub elo_seasons_used: Vec<SeasonYear>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DynastyResponse {
    #[serde(flatten)]
    pub report: DynastyReport,
    pub concentration: Option<ConcentrationComparison>,
    /// The titles read backwards against the real fields, as the Alpha Simulator reads
    /// them; `None` with `implied_note` saying why.
    pub implied: Option<Answer>,
    pub implied_note: Option<String>,
}

/// The rated NBA team-seasons of the window as the strength reading takes them.
fn field_rows<'a>(stats: &'a [TeamSeasonStat], req: &DynastyRequest) -> Vec<FieldRow<'a>> {
    stats
        .iter()
        .filter(|s| {
            s.season >= req.from_season
                && s.season <= req.to_season
                && League::of_code(&s.code) == League::Nba
        })
        .filter_map(|s| {
            s.elo_pre_playoffs.map(|elo| FieldRow {
                season: s.season,
                franchise_key: &s.franchise_key,
                code: &s.code,
                elo,
            })
        })
        .collect()
}

/// How long the page's reading waits for the one strength reader before answering busy:
/// a reading takes seconds, so a queue of a few is served and a stuck one is not waited on.
const READER_WAIT: std::time::Duration = std::time::Duration::from_secs(30);

/// Read the window's titles backwards from the team-seasons the report was built from:
/// the strength a team needed for them against the real fields. The reading waits its
/// turn for the one reader, up to [`READER_WAIT`], rather than answering busy at once, as
/// the page cannot retry. None with a note when there is no title, the window is too long,
/// the franchise has no rated season, the reader stays busy, or the reading failed.
pub async fn implied(
    app: &App,
    req: &DynastyRequest,
    titles: usize,
    stats: Vec<TeamSeasonStat>,
) -> (Option<Answer>, Option<String>) {
    if titles == 0 {
        return (
            None,
            Some("No title in the window, so there is nothing to read backwards.".to_string()),
        );
    }
    let record = Record {
        franchise: Some(req.franchise.clone()),
        from: req.from_season,
        to: req.to_season,
        titles,
    };
    if let Err(e) = strength::check(&record) {
        return (None, Some(e.to_string()));
    }
    if !stats.iter().any(|s| s.franchise_key == req.franchise) {
        return (
            None,
            Some("The franchise has no rated season in the store.".to_string()),
        );
    }
    let Ok(Ok(_slot)) = tokio::time::timeout(READER_WAIT, app.strength_slot.acquire()).await else {
        return (
            None,
            Some("The strength reader is busy with other records; run the analysis again in a moment.".to_string()),
        );
    };
    let read = tokio::task::spawn_blocking(move || strength::read_from(&stats, &record)).await;
    match read {
        Ok(Ok(answer)) => (Some(answer), None),
        Ok(Err(StrengthError::Invalid(m))) => (None, Some(m)),
        Ok(Err(e)) => {
            error!(error = %e, "the dynasty strength reading failed");
            (
                None,
                Some("The strength reading is not available right now.".to_string()),
            )
        }
        Err(e) => {
            error!(error = %e, "the dynasty strength reading did not finish");
            (
                None,
                Some("The strength reading is not available right now.".to_string()),
            )
        }
    }
}

/// The report and the team-seasons it read the strength from, for the implied reading.
pub async fn analyze(
    app: &App,
    req: &DynastyRequest,
) -> Result<(DynastyReport, Vec<TeamSeasonStat>), DynastyError> {
    let franchise = app
        .store
        .franchise_by_key(&req.franchise)
        .await?
        .ok_or_else(|| DynastyError::UnknownFranchise(req.franchise.clone()))?;
    let rows = app
        .store
        .franchise_seasons(franchise.id, req.from_season, req.to_season)
        .await?;
    let mut seasons = Vec::new();
    for ts in rows {
        let data = app.season_data(ts.season).await?;
        let team_count = {
            let mut ids: Vec<_> = data
                .games
                .iter()
                .flat_map(|g| [g.home_team_id, g.away_team_id])
                .collect();
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
    let stats = app.store.team_season_stats().await?;
    let strength = strength_seasons(
        &franchise.key,
        req.from_season,
        req.to_season,
        &field_rows(&stats, req),
    );
    let strength_summary = strength_summary(&strength);
    let report = DynastyReport {
        model_version: MODEL_VERSION.to_string(),
        franchise_key: franchise.key,
        franchise_name: franchise.name,
        from_season: req.from_season,
        to_season: req.to_season,
        alpha: req.alpha,
        seasons,
        actual_titles,
        comparisons,
        strength,
        strength_summary,
    };
    Ok((report, stats))
}

/// Compare league-wide title concentration under equal strength, a persistent alpha for the
/// selected franchise, and season-varying Elo championship probabilities (Engine A in
/// schedule mode, fed with Engine B's exact per-season probabilities).
pub async fn concentration(
    app: &App,
    req: &DynastyRequest,
    threshold: usize,
) -> Result<Option<ConcentrationComparison>, DynastyError> {
    let franchises = app.store.franchises(true).await?;
    let keys: Vec<String> = franchises.iter().map(|f| f.key.clone()).collect();
    let n = keys.len();
    let mut schedule: Vec<Vec<f64>> = Vec::new();
    let mut used = Vec::new();
    for season in SeasonYear::range(req.from_season, req.to_season) {
        let Ok(data) = app.season_data(season).await else {
            continue;
        };
        let Ok(r) = &data.reconstruction else {
            continue;
        };
        if !r.complete {
            continue;
        }
        let Some(exact) = req.model.exact(&r.bracket) else {
            continue;
        };
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
    let titles = app
        .store
        .title_counts(req.from_season, req.to_season)
        .await?;
    let actual_max = titles.values().copied().max().unwrap_or(0) as usize;
    let equal = vec![1.0; n];
    let mut alpha_vec = vec![1.0; n];
    if let Some(i) = keys.iter().position(|k| *k == req.franchise) {
        alpha_vec[i] = req.alpha;
    }
    let mut params = AlphaParams::equal(n, seasons, 50_000, threshold, 20240601);
    params.franchises = keys.clone();
    params.schedule = Some(schedule);
    let elo_result = tokio::task::spawn_blocking(move || alpha::run(&params, &Silent))
        .await
        .map_err(|e| StoreError::Query(format!("simulation task failed: {e}")))?;
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
