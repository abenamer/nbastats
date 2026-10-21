//! JSON API handlers.

use std::collections::HashMap;

use axum::extract::{Multipart, Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::domain::backtesting::BacktestParams;
use crate::config::APP_VERSION;
use crate::adapters::postgres as db;
use crate::domain::elo;
use crate::application::imports;
use crate::application::jobs::specs::{AlphaJob, BacktestJob, PlayoffJob, SeasonJob};
use crate::application::jobs::{self, Submit};
use crate::domain::models::{JobKind, SimulationJob};
use crate::domain::playoffs::bracket::ModelSpec;
use crate::domain::playoffs::season::SeasonParams;
use crate::domain::reference;
use crate::application::{dashboard, dynasty};
use crate::domain::simulation::alpha::AlphaParams;
use crate::domain::simulation::exact;

use super::error::{ApiError, ApiResult};
use super::State as AppState;

// ---------------------------------------------------------------------------------------
// Health and metadata
// ---------------------------------------------------------------------------------------

pub async fn healthz(State(app): State<AppState>) -> (StatusCode, Json<Value>) {
    let h = db::health(&app.pool).await;
    let status = if h.ok { StatusCode::OK } else { StatusCode::SERVICE_UNAVAILABLE };
    (status, Json(json!({ "ok": h.ok, "version": APP_VERSION })))
}

pub async fn health(State(app): State<AppState>) -> ApiResult<Value> {
    let h = db::health(&app.pool).await;
    let counts = db::analysis::table_counts(&app.pool).await?;
    let jobs = db::jobs::counts_by_status(&app.pool).await?;
    Ok(Json(json!({
        "app_version": APP_VERSION,
        "started_at": app.started_at,
        "uptime_seconds": (chrono::Utc::now() - app.started_at).num_seconds(),
        "database": h,
        "database_url": app.config.masked_database_url(),
        "data_dir": app.config.data_dir,
        "tables": counts.into_iter().collect::<HashMap<_, _>>(),
        "jobs": jobs.into_iter().collect::<HashMap<_, _>>(),
        "max_iterations": app.config.max_iterations,
        "rayon_threads": rayon::current_num_threads(),
    })))
}

pub async fn seasons(State(app): State<AppState>) -> ApiResult<Value> {
    let seasons = db::games::seasons(&app.pool).await?;
    Ok(Json(json!(seasons)))
}

pub async fn franchises(State(app): State<AppState>) -> ApiResult<Value> {
    Ok(Json(json!(db::teams::all_franchises(&app.pool).await?)))
}

// ---------------------------------------------------------------------------------------
// Elo explorer
// ---------------------------------------------------------------------------------------

fn check_season(season: i32) -> Result<(), ApiError> {
    if !(1947..=2100).contains(&season) {
        return Err(ApiError::bad_request("season out of range"));
    }
    Ok(())
}

pub async fn season_teams(State(app): State<AppState>, Path(season): Path<i32>) -> ApiResult<Value> {
    check_season(season)?;
    let rows = db::analysis::team_seasons(&app.pool, season).await?;
    if rows.is_empty() {
        return Err(ApiError::not_found(format!("no data for season {season}")));
    }
    Ok(Json(json!({ "season": season, "label": reference::season_label(season), "teams": rows })))
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

pub async fn season_elo_history(State(app): State<AppState>, Path(season): Path<i32>, Query(q): Query<HistoryQuery>) -> ApiResult<Value> {
    check_season(season)?;
    let data = app.season_data(season).await.map_err(|_| ApiError::not_found(format!("no data for season {season}")))?;
    let teams = app.teams().await?;
    let wanted: Vec<String> = q.teams.split(',').map(|s| s.trim().to_ascii_uppercase()).filter(|s| !s.is_empty()).collect();
    let mut by_team: HashMap<i32, TeamHistory> = HashMap::new();
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
    Ok(Json(json!({ "season": season, "series": series })))
}

#[derive(Deserialize)]
pub struct RangeQuery {
    pub from: Option<i32>,
    pub to: Option<i32>,
}

pub async fn franchise_elo(State(app): State<AppState>, Path(key): Path<String>, Query(q): Query<RangeQuery>) -> ApiResult<Value> {
    let f = db::teams::franchise_by_key(&app.pool, &key.to_ascii_uppercase())
        .await?
        .ok_or_else(|| ApiError::not_found("unknown franchise"))?;
    let from = q.from.unwrap_or(1947);
    let to = q.to.unwrap_or(2100);
    let points = db::games::franchise_elo_history(&app.pool, f.id, from, to).await?;
    let seasons = db::analysis::franchise_seasons(&app.pool, f.id, from, to).await?;
    Ok(Json(json!({ "franchise": f, "points": points, "seasons": seasons })))
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
        return Err(ApiError::bad_request("Elo ratings must be between 500 and 2500"));
    }
    if !(0.0..=400.0).contains(&q.home_advantage) || !(0.1..=3.0).contains(&q.scale) {
        return Err(ApiError::bad_request("home advantage must be 0-400 and scale 0.1-3"));
    }
    let neutral = q.home == "neutral";
    let a_home = q.home == "a";
    let game = elo::game_probability(q.elo_a, q.elo_b, a_home, neutral, q.home_advantage, q.scale);
    let series7 = elo::series_probability(&elo::series_game_probs(q.elo_a, q.elo_b, a_home || neutral, &elo::HOME_PATTERN_2_2_1_1_1, neutral, q.home_advantage, q.scale));
    let series5 = elo::series_probability(&elo::series_game_probs(q.elo_a, q.elo_b, a_home || neutral, &elo::HOME_PATTERN_2_2_1, neutral, q.home_advantage, q.scale));
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
    if q.franchises == 0 || q.franchises > 500 || q.seasons == 0 || q.seasons > 500 || q.threshold == 0 {
        return Err(ApiError::bad_request("franchises and seasons must be 1-500, threshold >= 1"));
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

pub async fn dashboard(State(app): State<AppState>, Query(q): Query<RangeQuery>) -> ApiResult<Value> {
    let latest = db::games::latest_season(&app.pool).await?.unwrap_or(2025);
    let from = q.from.unwrap_or(2004);
    let to = q.to.unwrap_or(latest);
    if to < from || from < 1947 || to > 2100 {
        return Err(ApiError::bad_request("invalid season range"));
    }
    Ok(Json(json!(dashboard::build(&app, from, to).await?)))
}

#[derive(Deserialize)]
pub struct BracketQuery {
    #[serde(default = "default_adv")]
    pub home_advantage: f64,
    #[serde(default = "default_scale")]
    pub scale: f64,
}

pub async fn season_bracket(State(app): State<AppState>, Path(season): Path<i32>, Query(q): Query<BracketQuery>) -> ApiResult<Value> {
    check_season(season)?;
    let model = ModelSpec::Elo { home_advantage: q.home_advantage, scale: q.scale, k: None };
    model.validate().map_err(ApiError::bad_request)?;
    let data = app.season_data(season).await.map_err(|_| ApiError::not_found(format!("no data for season {season}")))?;
    match &data.reconstruction {
        Ok(r) => {
            let exact = model.exact(&r.bracket);
            let series: Vec<Value> = r
                .series
                .iter()
                .map(|s| json!({ "team_a": s.team_a, "team_b": s.team_b, "wins": s.wins(), "round": s.round(), "winner": s.winner(), "first_date": s.first_date }))
                .collect();
            Ok(Json(json!({
                "season": season, "label": reference::season_label(season), "bracket": r.bracket, "complete": r.complete,
                "exact": exact, "series": series, "information_cutoff": data.pre_playoff_cutoff(), "model": model
            })))
        }
        Err(e) => Err(ApiError::not_found(e.to_string())),
    }
}

#[derive(Deserialize)]
pub struct DynastyQuery {
    pub franchise: String,
    pub from: i32,
    pub to: i32,
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

pub async fn dynasty(State(app): State<AppState>, Query(q): Query<DynastyQuery>) -> ApiResult<Value> {
    let req = dynasty::DynastyRequest {
        franchise: q.franchise.to_ascii_uppercase(),
        from_season: q.from,
        to_season: q.to,
        alpha: q.alpha,
        model: ModelSpec::Elo { home_advantage: q.home_advantage, scale: q.scale, k: None },
    };
    req.validate().map_err(ApiError::bad_request)?;
    if q.threshold == 0 || q.threshold > 100 {
        return Err(ApiError::bad_request("threshold must be 1-100"));
    }
    let report = dynasty::analyze(&app, &req).await.map_err(|e| ApiError::not_found(e.to_string()))?;
    let concentration = dynasty::concentration(&app, &req, q.threshold).await?;
    Ok(Json(json!(dynasty::DynastyResponse { report, concentration })))
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

pub async fn list_jobs(State(app): State<AppState>, Query(q): Query<JobsQuery>) -> ApiResult<Value> {
    let jobs = db::jobs::list(&app.pool, q.kind, q.limit.clamp(1, 500)).await?;
    Ok(Json(json!(jobs.into_iter().map(JobSummary::from).collect::<Vec<_>>())))
}

pub async fn scenarios(State(app): State<AppState>, Path(kind): Path<JobKind>) -> ApiResult<Value> {
    let jobs = db::jobs::scenarios(&app.pool, kind).await?;
    Ok(Json(json!(jobs.into_iter().map(JobSummary::from).collect::<Vec<_>>())))
}

pub async fn get_job(State(app): State<AppState>, Path(id): Path<Uuid>) -> ApiResult<SimulationJob> {
    let job = db::jobs::get(&app.pool, id).await?.ok_or_else(|| ApiError::not_found("job not found"))?;
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

pub async fn submit_alpha(State(app): State<AppState>, Json(body): Json<Named<AlphaParams>>) -> ApiResult<Value> {
    let job = AlphaJob(body.params).submit(&app, clean_name(body.name)?).await?;
    Ok(Json(json!(JobSummary::from(job))))
}

pub async fn submit_playoff(State(app): State<AppState>, Json(body): Json<Named<PlayoffJob>>) -> ApiResult<Value> {
    let job = body.params.submit(&app, clean_name(body.name)?).await?;
    Ok(Json(json!(JobSummary::from(job))))
}

pub async fn submit_season(State(app): State<AppState>, Json(body): Json<Named<SeasonParams>>) -> ApiResult<Value> {
    let job = SeasonJob(body.params).submit(&app, clean_name(body.name)?).await?;
    Ok(Json(json!(JobSummary::from(job))))
}

pub async fn submit_backtest(State(app): State<AppState>, Json(body): Json<Named<BacktestParams>>) -> ApiResult<Value> {
    let job = BacktestJob(body.params).submit(&app, clean_name(body.name)?).await?;
    Ok(Json(json!(JobSummary::from(job))))
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

pub async fn rename_job(State(app): State<AppState>, Path(id): Path<Uuid>, Json(body): Json<RenameBody>) -> ApiResult<Value> {
    let name = clean_name(body.name)?;
    if !db::jobs::rename(&app.pool, id, name.as_deref()).await? {
        return Err(ApiError::not_found("job not found"));
    }
    Ok(Json(json!({ "id": id, "name": name })))
}

pub async fn delete_job(State(app): State<AppState>, Path(id): Path<Uuid>) -> ApiResult<Value> {
    if !db::jobs::delete(&app.pool, id).await? {
        return Err(ApiError::bad_request("job not found or still running"));
    }
    Ok(Json(json!({ "id": id, "deleted": true })))
}

pub async fn latest_backtest(State(app): State<AppState>) -> ApiResult<Value> {
    let jobs = db::jobs::list(&app.pool, Some(JobKind::Backtest), 50).await?;
    match jobs.into_iter().find(|j| j.status == crate::domain::models::JobStatus::Completed) {
        Some(j) => Ok(Json(json!(j))),
        None => Err(ApiError::not_found("no completed backtest yet")),
    }
}

// ---------------------------------------------------------------------------------------
// Data management
// ---------------------------------------------------------------------------------------

pub async fn imports(State(app): State<AppState>) -> ApiResult<Value> {
    Ok(Json(json!(db::imports::list(&app.pool, 100).await?)))
}

#[derive(Deserialize)]
pub struct RefreshBody {
    #[serde(default)]
    pub force: bool,
}

pub async fn refresh(State(app): State<AppState>, body: Option<Json<RefreshBody>>) -> ApiResult<Value> {
    let force = body.map(|b| b.force).unwrap_or(false);
    let outcome = imports::refresh_from_url(&app.pool, &app.config.data_source_url, &app.config.data_dir, force)
        .await
        .map_err(|e| ApiError::bad_request(format!("refresh failed: {e}")))?;
    app.invalidate_cache();
    Ok(Json(json!(outcome)))
}

pub async fn upload(State(app): State<AppState>, mut multipart: Multipart) -> ApiResult<Value> {
    let mut file: Option<(String, Vec<u8>)> = None;
    let mut force = false;
    while let Some(field) = multipart.next_field().await.map_err(|e| ApiError::bad_request(e.to_string()))? {
        match field.name() {
            Some("file") => {
                let name = field.file_name().unwrap_or("upload.csv").to_string();
                let name: String = name.chars().filter(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_')).take(120).collect();
                let bytes = field.bytes().await.map_err(|e| ApiError::bad_request(e.to_string()))?;
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
    let bytes = imports::maybe_gunzip(bytes).map_err(|e| ApiError::bad_request(format!("could not decompress: {e}")))?;
    let outcome = imports::import_bytes(&app.pool, &bytes, "upload", None, Some(&name), force)
        .await
        .map_err(|e| ApiError::bad_request(format!("import failed: {e}")))?;
    app.invalidate_cache();
    Ok(Json(json!(outcome)))
}

pub async fn coverage(State(app): State<AppState>) -> ApiResult<Value> {
    let seasons = db::games::seasons(&app.pool).await?;
    let champs = db::analysis::championships(&app.pool, 1947, 2100).await?;
    let champ_by: HashMap<i32, &crate::domain::models::ChampionshipView> = champs.iter().map(|c| (c.season, c)).collect();
    let rows: Vec<Value> = seasons
        .iter()
        .map(|s| {
            let c = champ_by.get(&s.season);
            json!({
                "season": s.season, "label": s.label, "teams": s.team_count, "games": s.game_count,
                "playoff_games": s.playoff_game_count, "play_in_games": s.play_in_game_count, "cup_final_games": s.cup_final_game_count,
                "regular_season_end": s.regular_season_end, "playoffs_start": s.playoffs_start, "playoffs_end": s.playoffs_end,
                "complete": s.complete, "champion": c.map(|c| c.franchise_key.clone()), "champion_source": c.map(|c| c.source.clone()),
                "bracket_supported": s.season >= 2003,
            })
        })
        .collect();
    let present: Vec<i32> = seasons.iter().map(|s| s.season).collect();
    let missing: Vec<i32> = present.first().map(|&a| (a..=*present.last().unwrap()).filter(|s| !present.contains(s)).collect()).unwrap_or_default();
    Ok(Json(json!({ "seasons": rows, "missing_seasons": missing, "source_url": app.config.data_source_url })))
}
