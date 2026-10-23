//! JSON API handlers.

use std::collections::{HashMap, HashSet};

use axum::extract::{Multipart, Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::application::continuation::{self, ContinuationUseCaseError};
use crate::application::imports::{self, Provenance};
use crate::application::jobs::specs::{AlphaJob, BacktestJob, PlayoffJob, SeasonJob};
use crate::application::jobs::{self, Limits, Submit};
use crate::application::ports::{AnalysisStore, GameLog, ImportLog, JobStore, ResultsSource};
use crate::application::{dashboard, dynasty};
use crate::application::{results_sync, scheduler};
use crate::config::APP_VERSION;
use crate::domain::backtesting::BacktestParams;
use crate::domain::elo;
use crate::domain::ids::SeasonYear;
use crate::domain::models::{JobKind, SimulationJob};
use crate::domain::playoffs::bracket::ModelSpec;
use crate::domain::playoffs::season::SeasonParams;
use crate::domain::reference;
use crate::domain::simulation::alpha::AlphaParams;
use crate::domain::simulation::exact;

use super::error::{ApiError, ApiResult};
use super::State as AppState;

// ---------------------------------------------------------------------------------------
// Health and metadata
// ---------------------------------------------------------------------------------------

pub async fn healthz(State(app): State<AppState>) -> (StatusCode, Json<Value>) {
    let h = app.store.health().await;
    let status = if h.ok {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };
    (status, Json(json!({ "ok": h.ok, "version": APP_VERSION })))
}

pub async fn health(State(app): State<AppState>) -> ApiResult<Value> {
    let h = app.store.health().await;
    let counts = app.store.table_counts().await?;
    let jobs = app.store.job_counts().await?;
    Ok(Json(json!({
        "app_version": APP_VERSION,
        "started_at": app.started_at,
        "uptime_seconds": (chrono::Utc::now() - app.started_at).num_seconds(),
        "database": h,
        "database_url": app.config.masked_database_url(),
        "data_dir": app.config.data_dir,
        "tables": counts.into_iter().collect::<HashMap<_, _>>(),
        "jobs": jobs.into_iter().map(|(s, n)| (s.as_str(), n)).collect::<HashMap<_, _>>(),
        "max_iterations": app.config.max_iterations,
        "rayon_threads": rayon::current_num_threads(),
    })))
}

pub async fn seasons(State(app): State<AppState>) -> ApiResult<Value> {
    Ok(Json(json!(app.store.seasons().await?)))
}

/// Elo records: all-time, by decade, by league and league size, season and game records,
/// and the best and worst conferences and divisions.
pub async fn records(State(app): State<AppState>) -> ApiResult<Value> {
    let stats = app.store.team_season_stats().await?;
    use crate::domain::records::{GameRecords, MatchupOrder, LIST_LEN, MATCHUP_LEN};
    let games = GameRecords {
        swings: app.store.biggest_swings(LIST_LEN as i64).await?,
        greatest_matchups: app
            .store
            .matchups(MatchupOrder::Strongest, MATCHUP_LEN as i64)
            .await?,
        weakest_per_season: app.store.weakest_matchup_per_season().await?,
        champion_roads: app.store.champion_roads().await?,
    };
    let rosters = app.store.season_rosters().await?;
    Ok(Json(json!(crate::domain::records::compute(
        &stats, games, &rosters
    ))))
}

fn colors_json(key: &str) -> Value {
    let (primary, secondary) = reference::franchise_colors(key);
    json!({ "primary": primary, "secondary": secondary })
}

pub async fn franchises(State(app): State<AppState>) -> ApiResult<Value> {
    let rows: Vec<Value> = app
        .store
        .franchises(false)
        .await?
        .into_iter()
        .map(|f| {
            let colors = colors_json(&f.key);
            json!({ "id": f.id, "key": f.key, "name": f.name, "active": f.active, "colors": colors })
        })
        .collect();
    Ok(Json(json!(rows)))
}

// ---------------------------------------------------------------------------------------
// Elo explorer
// ---------------------------------------------------------------------------------------

pub async fn season_teams(
    State(app): State<AppState>,
    Path(season): Path<SeasonYear>,
) -> ApiResult<Value> {
    let rows = app.store.team_seasons(season).await?;
    if rows.is_empty() {
        return Err(ApiError::not_found(format!("no data for season {season}")));
    }
    Ok(Json(
        json!({ "season": season, "label": season.label(), "teams": rows }),
    ))
}

#[derive(Deserialize)]
pub struct HistoryQuery {
    /// Comma-separated team codes; empty = all teams.
    #[serde(default)]
    pub teams: String,
}

#[derive(Serialize)]
struct TeamHistory {
    code: String,
    name: String,
    franchise_key: String,
    points: Vec<(NaiveDate, f64, &'static str)>,
}

pub async fn season_elo_history(
    State(app): State<AppState>,
    Path(season): Path<SeasonYear>,
    Query(q): Query<HistoryQuery>,
) -> ApiResult<Value> {
    let data = app.season_data(season).await?;
    let teams = app.teams().await?;
    let wanted: Vec<String> = q
        .teams
        .split(',')
        .map(|s| s.trim().to_ascii_uppercase())
        .filter(|s| !s.is_empty())
        .collect();
    let mut by_team: HashMap<_, TeamHistory> = HashMap::new();
    for g in &data.games {
        if !g.is_played() {
            continue;
        }
        for tid in [g.home_team_id, g.away_team_id] {
            let Some(t) = teams.get(&tid) else { continue };
            if !wanted.is_empty() && !wanted.contains(&t.code) {
                continue;
            }
            let entry = by_team.entry(tid).or_insert_with(|| TeamHistory {
                code: t.code.clone(),
                name: t.name.clone(),
                franchise_key: t.franchise_key().to_string(),
                points: vec![],
            });
            let (_, post) = g.elo_of(tid);
            entry.points.push((g.game_date, post, g.game_type.as_str()));
        }
    }
    let mut series: Vec<TeamHistory> = by_team.into_values().collect();
    series.sort_by(|a, b| a.code.cmp(&b.code));
    // The schedule span, so the chart can show the part of the season that is not played yet.
    let dates = |played: Option<bool>| {
        data.games
            .iter()
            .filter(move |g| played.is_none_or(|p| g.is_played() == p))
            .map(|g| g.game_date)
    };
    let schedule = json!({
        "first": dates(None).min(),
        "last": dates(None).max(),
        "last_played": dates(Some(true)).max(),
        "remaining": data.games.iter().filter(|g| !g.is_played()).count(),
    });
    Ok(Json(
        json!({ "season": season, "series": series, "schedule": schedule }),
    ))
}

#[derive(Deserialize)]
pub struct RangeQuery {
    pub from: Option<SeasonYear>,
    pub to: Option<SeasonYear>,
}

pub async fn franchise_elo(
    State(app): State<AppState>,
    Path(key): Path<String>,
    Query(q): Query<RangeQuery>,
) -> ApiResult<Value> {
    let f = app
        .store
        .franchise_by_key(&key.to_ascii_uppercase())
        .await?
        .ok_or_else(|| ApiError::not_found("unknown franchise"))?;
    let from = q.from.unwrap_or(SeasonYear::MIN);
    let to = q.to.unwrap_or(SeasonYear::MAX);
    if to < from {
        return Err(ApiError::bad_request(
            "the season range ends before it starts",
        ));
    }
    let points = app.store.franchise_elo_history(f.id, from, to).await?;
    let seasons = app.store.franchise_seasons(f.id, from, to).await?;
    // Titles and lost Finals of this franchise, from the championships table (derived from
    // complete playoff data, or curated for seasons the archive covers only in part).
    let codes: HashSet<String> = app
        .store
        .teams()
        .await?
        .into_iter()
        .filter(|t| t.franchise_id == f.id)
        .map(|t| t.code)
        .collect();
    let championships: Vec<_> = app
        .store
        .championships(from, to)
        .await?
        .into_iter()
        .filter(|c| {
            c.franchise_key == f.key || c.runner_up_code.as_ref().is_some_and(|r| codes.contains(r))
        })
        .collect();
    let colors = colors_json(&f.key);
    Ok(Json(json!({
        "franchise": f,
        "colors": colors,
        "points": points,
        "seasons": seasons,
        "championships": championships,
    })))
}

#[derive(Deserialize)]
pub struct MatchupQuery {
    pub elo_a: f64,
    pub elo_b: f64,
    /// "a", "b" or "neutral"
    #[serde(default = "default_home")]
    pub home: String,
    #[serde(default = "default_adv")]
    pub home_advantage: f64,
    #[serde(default = "default_scale")]
    pub scale: f64,
}
fn default_home() -> String {
    "a".into()
}
fn default_adv() -> f64 {
    elo::DEFAULT_HOME_ADVANTAGE
}
fn default_scale() -> f64 {
    1.0
}

pub async fn matchup(Query(q): Query<MatchupQuery>) -> ApiResult<Value> {
    if !(500.0..=2500.0).contains(&q.elo_a) || !(500.0..=2500.0).contains(&q.elo_b) {
        return Err(ApiError::bad_request(
            "Elo ratings must be between 500 and 2500",
        ));
    }
    if !(0.0..=400.0).contains(&q.home_advantage) || !(0.1..=3.0).contains(&q.scale) {
        return Err(ApiError::bad_request(
            "home advantage must be 0-400 and scale 0.1-3",
        ));
    }
    let neutral = q.home == "neutral";
    let a_home = q.home == "a";
    let game = elo::game_probability(q.elo_a, q.elo_b, a_home, neutral, q.home_advantage, q.scale);
    let series7 = elo::series_probability(&elo::series_game_probs(
        q.elo_a,
        q.elo_b,
        a_home || neutral,
        &elo::HOME_PATTERN_2_2_1_1_1,
        neutral,
        q.home_advantage,
        q.scale,
    ));
    let series5 = elo::series_probability(&elo::series_game_probs(
        q.elo_a,
        q.elo_b,
        a_home || neutral,
        &elo::HOME_PATTERN_2_2_1,
        neutral,
        q.home_advantage,
        q.scale,
    ));
    let neutral_game = elo::win_probability(q.elo_a * q.scale, q.elo_b * q.scale);
    Ok(Json(json!({
        "p_game": game, "p_game_neutral": neutral_game, "p_series_best_of_7": series7, "p_series_best_of_5": series5,
        "elo_diff": q.elo_a - q.elo_b
    })))
}

// ---------------------------------------------------------------------------------------
// Exact alpha calculation (synchronous, for the validation box on the alpha page)
// ---------------------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct ExactQuery {
    pub franchises: usize,
    pub seasons: usize,
    pub threshold: usize,
}

pub async fn exact_alpha(Query(q): Query<ExactQuery>) -> ApiResult<Value> {
    if q.franchises == 0
        || q.franchises > 500
        || q.seasons == 0
        || q.seasons > 500
        || q.threshold == 0
    {
        return Err(ApiError::bad_request(
            "franchises and seasons must be 1-500, threshold >= 1",
        ));
    }
    let probs = vec![1.0; q.franchises];
    Ok(Json(json!({
        "p_any_at_least_k": exact::p_any_at_least(&probs, q.seasons, q.threshold),
        "p_franchise_at_least_k": exact::p_franchise_at_least(1.0 / q.franchises as f64, q.seasons, q.threshold),
        "max_titles_distribution": exact::max_titles_distribution(&probs, q.seasons),
    })))
}

// ---------------------------------------------------------------------------------------
// Dashboard, bracket, dynasty
// ---------------------------------------------------------------------------------------

pub async fn dashboard(
    State(app): State<AppState>,
    Query(q): Query<RangeQuery>,
) -> ApiResult<Value> {
    let latest = app.store.latest_season().await?.unwrap_or(SeasonYear::MAX);
    let from = q.from.unwrap_or(SeasonYear::of(2004));
    let to = q.to.unwrap_or(latest);
    if to < from {
        return Err(ApiError::bad_request(
            "the season range ends before it starts",
        ));
    }
    Ok(Json(json!(dashboard::build(&app.store, from, to).await?)))
}

#[derive(Deserialize)]
pub struct BracketQuery {
    #[serde(default = "default_adv")]
    pub home_advantage: f64,
    #[serde(default = "default_scale")]
    pub scale: f64,
}

pub async fn season_bracket(
    State(app): State<AppState>,
    Path(season): Path<SeasonYear>,
    Query(q): Query<BracketQuery>,
) -> ApiResult<Value> {
    let model = ModelSpec::Elo {
        home_advantage: q.home_advantage,
        scale: q.scale,
        k: None,
    };
    model.validate()?;
    let data = app.season_data(season).await?;
    match &data.reconstruction {
        Ok(r) => {
            let exact = model.exact(&r.bracket);
            let series: Vec<Value> = r
                .series
                .iter()
                .map(|s| json!({ "team_a": s.team_a, "team_b": s.team_b, "wins": s.wins(), "round": s.round(), "winner": s.winner(), "first_date": s.first_date }))
                .collect();
            Ok(Json(json!({
                "season": season, "label": season.label(), "bracket": r.bracket, "complete": r.complete,
                "exact": exact, "series": series, "information_cutoff": data.pre_playoff_cutoff(), "model": model
            })))
        }
        Err(e) => Err(ApiError::not_found(e.to_string())),
    }
}

#[derive(Deserialize)]
pub struct DynastyQuery {
    pub franchise: String,
    pub from: SeasonYear,
    pub to: SeasonYear,
    #[serde(default = "default_alpha")]
    pub alpha: f64,
    #[serde(default = "default_adv")]
    pub home_advantage: f64,
    #[serde(default = "default_scale")]
    pub scale: f64,
    #[serde(default = "default_threshold")]
    pub threshold: usize,
}
fn default_alpha() -> f64 {
    3.0
}
fn default_threshold() -> usize {
    4
}

pub async fn dynasty(
    State(app): State<AppState>,
    Query(q): Query<DynastyQuery>,
) -> ApiResult<Value> {
    let req = dynasty::DynastyRequest {
        franchise: q.franchise.to_ascii_uppercase(),
        from_season: q.from,
        to_season: q.to,
        alpha: q.alpha,
        model: ModelSpec::Elo {
            home_advantage: q.home_advantage,
            scale: q.scale,
            k: None,
        },
    };
    req.validate()?;
    if q.threshold == 0 || q.threshold > 100 {
        return Err(ApiError::bad_request("threshold must be 1-100"));
    }
    let report = dynasty::analyze(&app, &req).await?;
    let concentration = dynasty::concentration(&app, &req, q.threshold).await?;
    Ok(Json(json!(dynasty::DynastyResponse {
        report,
        concentration
    })))
}

// ---------------------------------------------------------------------------------------
// Jobs
// ---------------------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct JobsQuery {
    pub kind: Option<JobKind>,
    #[serde(default = "default_limit")]
    pub limit: i64,
}
fn default_limit() -> i64 {
    50
}

/// Job listing without the (potentially large) result payloads.
#[derive(Serialize)]
struct JobSummary {
    id: Uuid,
    kind: JobKind,
    name: Option<String>,
    status: crate::domain::models::JobStatus,
    progress: f64,
    error: Option<String>,
    seed: Option<i64>,
    app_version: String,
    model_version: String,
    attempts: i32,
    created_at: chrono::DateTime<chrono::Utc>,
    finished_at: Option<chrono::DateTime<chrono::Utc>>,
    params: Value,
}

impl From<SimulationJob> for JobSummary {
    fn from(j: SimulationJob) -> Self {
        Self {
            id: j.id,
            kind: j.kind,
            name: j.name,
            status: j.status,
            progress: j.progress,
            error: j.error,
            seed: j.seed,
            app_version: j.app_version,
            model_version: j.model_version,
            attempts: j.attempts,
            created_at: j.created_at,
            finished_at: j.finished_at,
            params: j.params,
        }
    }
}

pub async fn list_jobs(
    State(app): State<AppState>,
    Query(q): Query<JobsQuery>,
) -> ApiResult<Value> {
    let jobs = app.store.jobs(q.kind, q.limit.clamp(1, 500)).await?;
    Ok(Json(json!(jobs
        .into_iter()
        .map(JobSummary::from)
        .collect::<Vec<_>>())))
}

pub async fn scenarios(State(app): State<AppState>, Path(kind): Path<JobKind>) -> ApiResult<Value> {
    let jobs = app.store.scenarios(kind).await?;
    Ok(Json(json!(jobs
        .into_iter()
        .map(JobSummary::from)
        .collect::<Vec<_>>())))
}

pub async fn get_job(
    State(app): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<SimulationJob> {
    let job = app
        .store
        .job(id)
        .await?
        .ok_or_else(|| ApiError::not_found("job not found"))?;
    Ok(Json(job))
}

#[derive(Deserialize)]
pub struct Named<T> {
    pub name: Option<String>,
    #[serde(flatten)]
    pub params: T,
}

fn clean_name(name: Option<String>) -> Result<Option<String>, ApiError> {
    match name {
        None => Ok(None),
        Some(n) => {
            let n = n.trim().to_string();
            if n.is_empty() {
                Ok(None)
            } else if n.chars().count() > 120 {
                Err(ApiError::bad_request("name too long (max 120 characters)"))
            } else {
                Ok(Some(n))
            }
        }
    }
}

fn limits(app: &AppState) -> Limits {
    Limits {
        max_iterations: app.config.max_iterations,
        max_work_units: app.config.max_work_units,
    }
}

async fn queue<T: Submit>(app: &AppState, spec: T, name: Option<String>) -> ApiResult<Value> {
    let job = spec
        .submit(&app.store, &limits(app), clean_name(name)?)
        .await?;
    app.job_notify.notify_one();
    Ok(Json(json!(JobSummary::from(job))))
}

pub async fn submit_alpha(
    State(app): State<AppState>,
    Json(body): Json<Named<AlphaParams>>,
) -> ApiResult<Value> {
    queue(&app, AlphaJob(body.params), body.name).await
}

pub async fn submit_playoff(
    State(app): State<AppState>,
    Json(body): Json<Named<PlayoffJob>>,
) -> ApiResult<Value> {
    queue(&app, body.params, body.name).await
}

pub async fn submit_season(
    State(app): State<AppState>,
    Json(body): Json<Named<SeasonParams>>,
) -> ApiResult<Value> {
    queue(&app, SeasonJob(body.params), body.name).await
}

pub async fn submit_backtest(
    State(app): State<AppState>,
    Json(body): Json<Named<BacktestParams>>,
) -> ApiResult<Value> {
    queue(&app, BacktestJob(body.params), body.name).await
}

pub async fn retry_job(State(app): State<AppState>, Path(id): Path<Uuid>) -> ApiResult<Value> {
    match jobs::retry(&app, id).await? {
        Some(s) => Ok(Json(json!({ "id": id, "status": s }))),
        None => Err(ApiError::bad_request("job is not in a retryable state")),
    }
}

pub async fn cancel_job(State(app): State<AppState>, Path(id): Path<Uuid>) -> ApiResult<Value> {
    match jobs::cancel(&app, id).await? {
        Some(s) => Ok(Json(json!({ "id": id, "status": s }))),
        None => Err(ApiError::bad_request("job is not running or queued")),
    }
}

#[derive(Deserialize)]
pub struct RenameBody {
    pub name: Option<String>,
}

pub async fn rename_job(
    State(app): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<RenameBody>,
) -> ApiResult<Value> {
    let name = clean_name(body.name)?;
    if !app.store.rename_job(id, name.as_deref()).await? {
        return Err(ApiError::not_found("job not found"));
    }
    Ok(Json(json!({ "id": id, "name": name })))
}

pub async fn delete_job(State(app): State<AppState>, Path(id): Path<Uuid>) -> ApiResult<Value> {
    if !app.store.delete_job(id).await? {
        return Err(ApiError::bad_request("job not found or still running"));
    }
    Ok(Json(json!({ "id": id, "deleted": true })))
}

pub async fn latest_backtest(State(app): State<AppState>) -> ApiResult<Value> {
    let jobs = app.store.jobs(Some(JobKind::Backtest), 50).await?;
    match jobs
        .into_iter()
        .find(|j| j.status == crate::domain::models::JobStatus::Completed)
    {
        Some(j) => Ok(Json(json!(j))),
        None => Err(ApiError::not_found("no completed backtest yet")),
    }
}

// ---------------------------------------------------------------------------------------
// Data management
// ---------------------------------------------------------------------------------------

pub async fn imports(State(app): State<AppState>) -> ApiResult<Value> {
    Ok(Json(json!(app.store.imports(100).await?)))
}

#[derive(Deserialize)]
pub struct RefreshBody {
    #[serde(default)]
    pub force: bool,
}

pub async fn refresh(
    State(app): State<AppState>,
    body: Option<Json<RefreshBody>>,
) -> ApiResult<Value> {
    let force = body.map(|b| b.force).unwrap_or(false);
    let outcome = imports::refresh_from_url(
        &app.store,
        &app.source,
        &app.config.data_source_url,
        &app.config.data_dir,
        force,
    )
    .await?;
    app.invalidate_cache();
    Ok(Json(json!(outcome)))
}

pub async fn upload(State(app): State<AppState>, mut multipart: Multipart) -> ApiResult<Value> {
    let mut file: Option<(String, Vec<u8>)> = None;
    let mut force = false;
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| ApiError::bad_request(e.to_string()))?
    {
        match field.name() {
            Some("file") => {
                let name = field.file_name().unwrap_or("upload.csv").to_string();
                let name: String = name
                    .chars()
                    .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
                    .take(120)
                    .collect();
                let bytes = field
                    .bytes()
                    .await
                    .map_err(|e| ApiError::bad_request(e.to_string()))?;
                file = Some((name, bytes.to_vec()));
            }
            Some("force") => {
                let v = field.text().await.unwrap_or_default();
                force = matches!(v.as_str(), "1" | "true" | "on");
            }
            _ => {}
        }
    }
    let (name, bytes) = file.ok_or_else(|| ApiError::bad_request("missing 'file' field"))?;
    let bytes = imports::maybe_gunzip(bytes)
        .map_err(|e| ApiError::bad_request(format!("could not decompress: {e}")))?;
    let outcome = imports::import_bytes(
        &app.store,
        &bytes,
        Provenance {
            source: "upload",
            source_url: None,
            file_name: Some(&name),
        },
        force,
    )
    .await?;
    app.invalidate_cache();
    Ok(Json(json!(outcome)))
}

#[derive(Deserialize)]
pub struct ProjectBody {
    pub season: SeasonYear,
}

fn continuation_error(e: ContinuationUseCaseError) -> ApiError {
    match e {
        ContinuationUseCaseError::Store(s) => s.into(),
        other => ApiError::bad_request(other.to_string()),
    }
}

/// Preseason projection of a season the dataset does not cover (reverted ratings, synthetic schedule).
pub async fn project_season(
    State(app): State<AppState>,
    Json(body): Json<ProjectBody>,
) -> ApiResult<Value> {
    let outcome = continuation::project_season(&app.store, body.season)
        .await
        .map_err(continuation_error)?;
    app.invalidate_cache();
    Ok(Json(json!(outcome)))
}

/// Pull a season's results from BALLDONTLIE and compute the Elo.
pub async fn sync_results(
    State(app): State<AppState>,
    Json(body): Json<ProjectBody>,
) -> ApiResult<Value> {
    match results_sync::start(app.clone(), body.season) {
        Ok(()) => Ok(Json(
            json!({ "started": true, "season": body.season, "status_url": "/api/data/sync/status" }),
        )),
        Err(results_sync::StartError::NotConfigured) => Err(ApiError::bad_request(
            results_sync::StartError::NotConfigured.to_string(),
        )),
        Err(e @ results_sync::StartError::AlreadyRunning) => Err(ApiError::conflict(e.to_string())),
    }
}

/// Live state of the results sync (manual or scheduled) for the Data page to poll.
pub async fn sync_status(State(app): State<AppState>) -> ApiResult<Value> {
    Ok(Json(json!(app.sync_phase())))
}

/// Remove a season that holds only computed games (or no games at all).
pub async fn remove_season(
    State(app): State<AppState>,
    Path(season): Path<SeasonYear>,
) -> ApiResult<Value> {
    let deleted = continuation::remove_season(&app.store, season)
        .await
        .map_err(continuation_error)?;
    app.invalidate_cache();
    Ok(Json(json!({ "season": season, "games_deleted": deleted })))
}

/// Results or schedule CSV for a season beyond the dataset; the application computes the Elo.
pub async fn upload_results(
    State(app): State<AppState>,
    mut multipart: Multipart,
) -> ApiResult<Value> {
    let mut file: Option<(String, Vec<u8>)> = None;
    let mut season: Option<SeasonYear> = None;
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| ApiError::bad_request(e.to_string()))?
    {
        match field.name() {
            Some("file") => {
                let name: String = field
                    .file_name()
                    .unwrap_or("results.csv")
                    .chars()
                    .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
                    .take(120)
                    .collect();
                let bytes = field
                    .bytes()
                    .await
                    .map_err(|e| ApiError::bad_request(e.to_string()))?;
                file = Some((name, bytes.to_vec()));
            }
            Some("season") => {
                let v = field.text().await.unwrap_or_default();
                season = Some(
                    v.trim()
                        .parse()
                        .map_err(|_| ApiError::bad_request("invalid season"))?,
                );
            }
            _ => {}
        }
    }
    let (name, bytes) = file.ok_or_else(|| ApiError::bad_request("missing 'file' field"))?;
    let season = season
        .ok_or_else(|| ApiError::bad_request("missing 'season' field (end year, e.g. 2026)"))?;
    let bytes = imports::maybe_gunzip(bytes)
        .map_err(|e| ApiError::bad_request(format!("could not decompress: {e}")))?;
    let outcome = continuation::import_results(&app.store, season, &bytes, Some(&name))
        .await
        .map_err(continuation_error)?;
    app.invalidate_cache();
    Ok(Json(json!(outcome)))
}

pub async fn coverage(State(app): State<AppState>) -> ApiResult<Value> {
    let seasons = app.store.seasons().await?;
    let champs = app
        .store
        .championships(SeasonYear::MIN, SeasonYear::MAX)
        .await?;
    let champ_by: HashMap<SeasonYear, &crate::domain::models::ChampionshipView> =
        champs.iter().map(|c| (c.season, c)).collect();
    let rows: Vec<Value> = seasons
        .iter()
        .map(|s| {
            let c = champ_by.get(&s.season);
            json!({
                "season": s.season, "label": s.label, "teams": s.team_count, "games": s.game_count,
                "playoff_games": s.playoff_game_count, "play_in_games": s.play_in_game_count, "cup_final_games": s.cup_final_game_count,
                "computed_games": s.computed_game_count, "scheduled_games": s.scheduled_game_count,
                "regular_season_end": s.regular_season_end, "playoffs_start": s.playoffs_start, "playoffs_end": s.playoffs_end,
                "complete": s.complete, "champion": c.map(|c| c.franchise_key.clone()), "champion_source": c.map(|c| c.source.clone()),
                "bracket_supported": s.season >= SeasonYear::FIRST_MODERN,
                "removable": s.game_count == 0 || s.computed_game_count == s.game_count,
            })
        })
        .collect();
    let present: Vec<SeasonYear> = seasons.iter().map(|s| s.season).collect();
    let missing: Vec<SeasonYear> = match (present.first(), present.last()) {
        (Some(&a), Some(&b)) => SeasonYear::range(a, b)
            .filter(|s| !present.contains(s))
            .collect(),
        _ => vec![],
    };
    // The first season after the last one with data, as the default target for a projection.
    let next_season = present.last().and_then(|s| s.next());
    Ok(Json(json!({
        "seasons": rows,
        "missing_seasons": missing,
        "source_url": app.config.data_source_url,
        "next_season": next_season,
        "results_example": continuation::results_example(),
        "results_source": app.results.as_ref().map(|r| json!({
            "name": r.name(),
            "base_url": r.base_url(),
            "requests_per_minute": r.requests_per_minute(),
        })),
        "results_sync": app.sync_schedule.map(|s| {
            let now = chrono::Utc::now();
            json!({
                "time": s.time.format("%H:%M").to_string(),
                "zone": s.zone.name(),
                "season": scheduler::current_season(now.with_timezone(&s.zone).date_naive()),
                "next_run": s.next_run(now),
            })
        }),
    })))
}
