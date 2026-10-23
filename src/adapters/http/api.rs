//! JSON API handlers.

use std::collections::{BTreeMap, HashMap, HashSet};

use axum::extract::{Multipart, Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::application::ask::{self, AskError};
use crate::application::continuation::{self, ContinuationUseCaseError};
use crate::application::imports::{self, Provenance};
use crate::application::jobs::specs::{AlphaJob, BacktestJob, PlayoffJob, SeasonJob};
use crate::application::jobs::{self, Limits, Submit};
use crate::application::ports::{
    AnalysisStore, GameLog, ImportLog, JobStore, LanguageModel, ModelError, ResultsSource,
};
use crate::application::reports::{self, Fetching, PlayedGame, ReportError};
use crate::application::{dashboard, dynasty};
use crate::application::{results_sync, scheduler};
use crate::config::APP_VERSION;
use crate::domain::backtesting::BacktestParams;
use crate::domain::elo;
use crate::domain::eras::{
    self, Dimension, Facet, GroupQuery, Scope, DYNASTY_GAP, DYNASTY_TITLES, MAX_DIMENSIONS,
    MAX_FACETS,
};
use crate::domain::ids::SeasonYear;
use crate::domain::models::{JobKind, SimulationJob};
use crate::domain::playoffs::bracket::ModelSpec;
use crate::domain::playoffs::season::SeasonParams;
use crate::domain::records::{league_sizes, League, TeamSeasonStat};
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

/// The parameters the records endpoints share, read as repeated pairs so `facet` may appear
/// more than once. Every value is text so a bad value answers with a JSON message, not
/// axum's plain text.
#[derive(Debug, Default)]
struct EraParams {
    facets: Vec<Facet>,
    /// Comma-separated dimensions of a group query, for example `cba,size`.
    by: String,
    league: Option<String>,
    from: Option<String>,
    to: Option<String>,
    dynasty_titles: Option<String>,
    dynasty_gap: Option<String>,
}

fn era_params(pairs: &[(String, String)]) -> Result<EraParams, ApiError> {
    let mut p = EraParams::default();
    for (key, value) in pairs {
        match key.as_str() {
            "facet" => {
                let f: Facet = value.parse().map_err(ApiError::bad_request)?;
                if !p.facets.contains(&f) {
                    p.facets.push(f);
                }
                if p.facets.len() > MAX_FACETS {
                    return Err(ApiError::bad_request(format!(
                        "at most {MAX_FACETS} facets"
                    )));
                }
            }
            "by" => p.by = value.clone(),
            "league" => p.league = Some(value.clone()),
            "from" => p.from = Some(value.clone()),
            "to" => p.to = Some(value.clone()),
            "dynasty_titles" => p.dynasty_titles = Some(value.clone()),
            "dynasty_gap" => p.dynasty_gap = Some(value.clone()),
            _ => {}
        }
    }
    Ok(p)
}

fn whole_number(value: Option<&str>, what: &str) -> Result<Option<i64>, ApiError> {
    match value.map(str::trim).filter(|v| !v.is_empty()) {
        None => Ok(None),
        Some(v) => v
            .parse::<i64>()
            .map(Some)
            .map_err(|_| ApiError::bad_request(format!("{what} must be a whole number"))),
    }
}

fn season_param(value: Option<&str>, what: &str) -> Result<Option<SeasonYear>, ApiError> {
    whole_number(value, what)?
        .map(|y| {
            i32::try_from(y)
                .ok()
                .and_then(|y| SeasonYear::try_new(y).ok())
                .ok_or_else(|| {
                    ApiError::bad_request(format!(
                        "{what} must be a season end year between 1947 and 2100"
                    ))
                })
        })
        .transpose()
}

/// The dynasty thresholds of a group query, clamped to the ranges the page offers. They
/// shape the group rows only: the dynasty facets, their listing and the scope of every
/// endpoint use the defaults, so the three endpoints always agree on what a dynasty is.
fn dynasty_thresholds(p: &EraParams) -> Result<(usize, usize), ApiError> {
    Ok((
        whole_number(p.dynasty_titles.as_deref(), "dynasty_titles")?
            .map_or(DYNASTY_TITLES, |n| n.clamp(1, 20) as usize),
        whole_number(p.dynasty_gap.as_deref(), "dynasty_gap")?
            .map_or(DYNASTY_GAP, |n| n.clamp(0, 20) as usize),
    ))
}

/// The team-seasons, the schedule rosters and the league sizes, with the scope the facets
/// select (`None` without facets) at the default dynasty thresholds. An unknown label
/// answers 400.
struct EraData {
    stats: Vec<TeamSeasonStat>,
    rosters: Vec<(SeasonYear, String)>,
    sizes: BTreeMap<(SeasonYear, League), usize>,
    scope: Option<Scope>,
}

async fn era_data(app: &AppState, p: &EraParams) -> Result<EraData, ApiError> {
    let stats = app.store.team_season_stats().await?;
    let rosters = app.store.season_rosters().await?;
    let sizes = league_sizes(&stats, &rosters);
    let scope = if p.facets.is_empty() {
        None
    } else {
        Some(
            eras::scope(&p.facets, &stats, &sizes, DYNASTY_TITLES, DYNASTY_GAP)
                .map_err(ApiError::bad_request)?,
        )
    };
    Ok(EraData {
        stats,
        rosters,
        sizes,
        scope,
    })
}

/// Elo records: all-time, by decade, by league and league size, season and game records,
/// and the best and worst conferences and divisions. With `facet` parameters, the same
/// tables over the seasons the facets select, and `scope` describes the selection. Other
/// parameters are ignored.
pub async fn records(
    State(app): State<AppState>,
    Query(pairs): Query<Vec<(String, String)>>,
) -> ApiResult<Value> {
    use crate::domain::records::{GameRecords, MatchupOrder, LIST_LEN, MATCHUP_LEN};
    let params = era_params(&pairs)?;
    let EraData {
        mut stats,
        mut rosters,
        scope,
        ..
    } = era_data(&app, &params).await?;
    let season_scope = scope.as_ref().map(Scope::season_scope);
    if let Some(sc) = &scope {
        stats.retain(|s| sc.contains(s.season, League::of_code(&s.code)));
        rosters.retain(|(season, code)| sc.contains(*season, League::of_code(code)));
    }
    let sc = season_scope.as_ref();
    let games = GameRecords {
        swings: app.store.biggest_swings(sc, LIST_LEN as i64).await?,
        greatest_matchups: app
            .store
            .matchups(sc, MatchupOrder::Strongest, MATCHUP_LEN as i64)
            .await?,
        weakest_per_season: app.store.weakest_matchup_per_season(sc).await?,
        champion_roads: app.store.champion_roads(sc).await?,
    };
    let mut answer = json!(crate::domain::records::compute(&stats, games, &rosters));
    answer["scope"] = json!(scope);
    Ok(Json(answer))
}

/// The facets the data offers: every dimension with its labels, spans and season counts,
/// the dynasties at the default thresholds. Takes no parameters.
pub async fn records_facets(State(app): State<AppState>) -> ApiResult<Value> {
    let stats = app.store.team_season_stats().await?;
    let rosters = app.store.season_rosters().await?;
    let sizes = league_sizes(&stats, &rosters);
    Ok(Json(json!({
        "dimensions": eras::facets(&stats, &sizes, DYNASTY_TITLES, DYNASTY_GAP),
        "max_facets": MAX_FACETS,
    })))
}

/// Era builder: the records of every group of seasons the chosen dimensions make, within
/// the facets when there are any.
pub async fn records_groups(
    State(app): State<AppState>,
    Query(pairs): Query<Vec<(String, String)>>,
) -> ApiResult<Value> {
    let params = era_params(&pairs)?;
    let mut dimensions: Vec<Dimension> = Vec::new();
    for part in params
        .by
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        let d: Dimension = part.parse().map_err(ApiError::bad_request)?;
        if !dimensions.contains(&d) {
            dimensions.push(d);
        }
    }
    if dimensions.len() > MAX_DIMENSIONS {
        return Err(ApiError::bad_request(format!(
            "at most {MAX_DIMENSIONS} dimensions"
        )));
    }
    let league = match params
        .league
        .as_deref()
        .map(|l| l.trim().to_ascii_lowercase())
    {
        None => None,
        Some(l) if l.is_empty() || l == "all" => None,
        Some(l) if l == "nba" => Some(League::Nba),
        Some(l) if l == "aba" => Some(League::Aba),
        Some(other) => {
            let shown: String = other.chars().take(40).collect();
            return Err(ApiError::bad_request(format!(
                "unknown league {shown:?}; use nba, aba or all"
            )));
        }
    };
    let from = season_param(params.from.as_deref(), "from")?;
    let to = season_param(params.to.as_deref(), "to")?;
    if let (Some(f), Some(t)) = (from, to) {
        if f > t {
            return Err(ApiError::bad_request("from must not be after to"));
        }
    }
    let (dynasty_titles, dynasty_gap) = dynasty_thresholds(&params)?;
    let query = GroupQuery {
        dimensions,
        league,
        from,
        to,
        dynasty_titles,
        dynasty_gap,
    };
    let EraData {
        stats,
        sizes,
        scope,
        ..
    } = era_data(&app, &params).await?;
    Ok(Json(json!(eras::group(
        &query,
        &stats,
        &sizes,
        scope.as_ref()
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

/// The strength a title record implies, for the backward view of the Alpha Simulator. Every
/// field is text so a bad value answers with a JSON message.
#[derive(Deserialize)]
pub struct StrengthQuery {
    pub franchise: Option<String>,
    pub from: Option<String>,
    pub to: Option<String>,
    pub titles: Option<String>,
}

pub async fn strength(
    State(app): State<AppState>,
    Query(q): Query<StrengthQuery>,
) -> ApiResult<Value> {
    use crate::application::strength::{read_record, Record, StrengthError};
    let from = season_param(q.from.as_deref(), "from")?
        .ok_or_else(|| ApiError::bad_request("from is required"))?;
    let to = season_param(q.to.as_deref(), "to")?
        .ok_or_else(|| ApiError::bad_request("to is required"))?;
    let titles = whole_number(q.titles.as_deref(), "titles")?
        .ok_or_else(|| ApiError::bad_request("titles is required"))?;
    let titles = usize::try_from(titles)
        .map_err(|_| ApiError::bad_request("titles must be a whole number of titles"))?;
    let franchise = match q.franchise.map(|k| k.trim().to_ascii_uppercase()) {
        None => None,
        Some(k) if k.is_empty() => None,
        Some(k) if k.len() <= 8 && k.chars().all(|c| c.is_ascii_alphanumeric()) => Some(k),
        Some(_) => return Err(ApiError::bad_request("franchise must be a franchise key")),
    };
    let record = Record {
        franchise,
        from,
        to,
        titles,
    };
    match read_record(&app.store, &app.strength_slot, record).await {
        Ok(answer) => Ok(Json(json!(answer))),
        Err(StrengthError::Invalid(msg)) => Err(ApiError::bad_request(msg)),
        Err(StrengthError::Busy) => Err(ApiError::Busy(
            "another record is being read; try again in a moment".to_string(),
        )),
        Err(StrengthError::Store(e)) => Err(e.into()),
    }
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
    let _guard = import_slot(&app)?;
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

/// An archive import recomputes every season, so it never runs beside a results sync: both
/// take the sync lock, and the one that finds it taken answers 409.
fn import_slot(app: &AppState) -> Result<tokio::sync::OwnedMutexGuard<()>, ApiError> {
    app.sync_lock.clone().try_lock_owned().map_err(|_| {
        ApiError::conflict("a results sync is running; import again when it has finished")
    })
}

pub async fn upload(State(app): State<AppState>, mut multipart: Multipart) -> ApiResult<Value> {
    let _guard = import_slot(&app)?;
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
                "computed_games": s.computed_game_count, "archive_games": s.archive_game_count, "scheduled_games": s.scheduled_game_count,
                "regular_season_end": s.regular_season_end, "playoffs_start": s.playoffs_start, "playoffs_end": s.playoffs_end,
                "complete": s.complete, "champion": c.map(|c| c.franchise_key.clone()), "champion_source": c.map(|c| c.source.clone()),
                "bracket_supported": s.season >= SeasonYear::FIRST_MODERN,
                "removable": s.archive_game_count == 0,
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

// ---------------------------------------------------------------------------------------
// Game reports
// ---------------------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct ReportsQuery {
    /// Comma-separated dates, YYYY-MM-DD.
    #[serde(default)]
    pub dates: String,
}

/// The report links for the games of the given dates (at most 40). Dates not looked up yet
/// are fetched a few per request; the rest come back as `pending` for a later request.
pub async fn game_reports(
    State(app): State<AppState>,
    Query(q): Query<ReportsQuery>,
) -> ApiResult<reports::Reports> {
    let mut dates: Vec<NaiveDate> = Vec::new();
    for part in q.dates.split(',').map(str::trim).filter(|s| !s.is_empty()) {
        if dates.len() >= reports::MAX_DATES {
            return Err(ApiError::bad_request(ReportError::TooManyDates.to_string()));
        }
        let d = NaiveDate::parse_from_str(part, "%Y-%m-%d")
            .map_err(|_| ApiError::bad_request("dates must be YYYY-MM-DD, comma-separated"))?;
        if !dates.contains(&d) {
            dates.push(d);
        }
    }
    // The played games of each date, by season.
    let teams = app.teams().await?;
    let mut games: std::collections::BTreeMap<NaiveDate, Vec<PlayedGame>> = Default::default();
    let mut seasons: Vec<SeasonYear> = dates
        .iter()
        .map(|d| scheduler::season_of_date(*d))
        .collect();
    seasons.sort_unstable();
    seasons.dedup();
    for season in seasons {
        let Ok(data) = app.season_data(season).await else {
            continue;
        };
        for g in data
            .games
            .iter()
            .filter(|g| g.is_played() && dates.contains(&g.game_date))
        {
            if let (Some(h), Some(a)) = (teams.get(&g.home_team_id), teams.get(&g.away_team_id)) {
                games.entry(g.game_date).or_default().push(PlayedGame {
                    home_code: h.code.clone(),
                    away_code: a.code.clone(),
                });
            }
        }
    }
    let team_list: Vec<_> = teams.values().cloned().collect();
    // One lookup at a time: a request that finds the slot taken answers from the store.
    let slot = app.report_slot.try_acquire().ok();
    let fetching = match (&app.reports, &slot) {
        (None, _) => Fetching::Off,
        (Some(_), None) => Fetching::Busy,
        (Some(source), Some(_)) => Fetching::With(source),
    };
    let out = reports::for_dates(&app.store, fetching, &team_list, &games, &dates)
        .await
        .map_err(|e| match e {
            ReportError::TooManyDates => ApiError::bad_request(e.to_string()),
            ReportError::Store(s) => s.into(),
        })?;
    Ok(Json(out))
}

// ---------------------------------------------------------------------------------------
// Question box
// ---------------------------------------------------------------------------------------

pub async fn ask_status(State(app): State<AppState>) -> ApiResult<Value> {
    Ok(Json(json!({
        "enabled": app.model.is_some(),
        "model": app.model.as_ref().map(|m| m.model_name()),
        "max_question_chars": ask::MAX_QUESTION_CHARS,
    })))
}

#[derive(Deserialize)]
pub struct AskBody {
    #[serde(default)]
    pub question: String,
}

/// Answer a question with the configured language model and the read-only tools. At most
/// `ASK_SLOTS` questions run at once; the model is never given write access.
pub async fn ask(State(app): State<AppState>, Json(body): Json<AskBody>) -> ApiResult<ask::Answer> {
    let model = app.model.as_ref().ok_or_else(|| {
        ApiError::NotConfigured(
            "the question box needs OPENROUTER_API_KEY (see .env.example)".to_string(),
        )
    })?;
    let _slot = app.ask_slots.try_acquire().map_err(|_| {
        ApiError::Busy("another question is being answered; try again in a moment".to_string())
    })?;
    let answer = ask::answer(&app.store, model, &body.question, app.today())
        .await
        .map_err(ask_error)?;
    Ok(Json(answer))
}

fn ask_error(e: AskError) -> ApiError {
    match e {
        AskError::Empty | AskError::TooLong => ApiError::BadRequest(e.to_string()),
        AskError::Timeout => {
            ApiError::Upstream("the language model did not answer in time".to_string())
        }
        AskError::NoAnswer => ApiError::Upstream("the language model gave no answer".to_string()),
        AskError::Model(ModelError::NotConfigured(m)) => ApiError::NotConfigured(m),
        AskError::Model(ModelError::Rejected { status, message }) => {
            tracing::warn!(status, %message, "language model rejected the request");
            ApiError::Upstream(format!(
                "the language model service answered with status {status}; see the log"
            ))
        }
        AskError::Model(ModelError::Transport(m) | ModelError::InvalidResponse(m)) => {
            tracing::warn!(error = %m, "language model request failed");
            ApiError::Upstream("the language model could not be reached; see the log".to_string())
        }
        AskError::Store(s) => s.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pairs(list: &[(&str, &str)]) -> Vec<(String, String)> {
        list.iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn era_params_read_repeated_facets_and_scalar_keys() {
        let p = era_params(&pairs(&[
            ("facet", "player:LeBron James"),
            ("facet", "rule:Hand-check ban"),
            ("facet", "player:LeBron James"),
            ("by", "cba"),
            ("by", "rule"),
            ("league", "nba"),
            ("unknown", "ignored"),
            ("dynasty_titles", "4"),
        ]))
        .unwrap();
        let shown: Vec<String> = p.facets.iter().map(ToString::to_string).collect();
        assert_eq!(shown, ["player:LeBron James", "rule:Hand-check ban"]);
        // A repeated scalar key keeps its last value; an unknown key is ignored.
        assert_eq!(p.by, "rule");
        assert_eq!(p.league.as_deref(), Some("nba"));
        assert_eq!(dynasty_thresholds(&p).unwrap(), (4, DYNASTY_GAP));
        assert_eq!(
            dynasty_thresholds(&EraParams::default()).unwrap(),
            (DYNASTY_TITLES, DYNASTY_GAP)
        );
        assert_eq!(
            dynasty_thresholds(&era_params(&pairs(&[("dynasty_gap", "99")])).unwrap()).unwrap(),
            (DYNASTY_TITLES, 20)
        );
    }

    #[test]
    fn era_params_reject_bad_facets_and_too_many() {
        for bad in ["LeBron James", "salary:cap", "player:", "player:   "] {
            assert!(era_params(&pairs(&[("facet", bad)])).is_err(), "{bad}");
        }
        let nine: Vec<(String, String)> = (0..9)
            .map(|i| ("facet".to_string(), format!("decade:{}0s", 194 + i)))
            .collect();
        match era_params(&nine) {
            Err(ApiError::BadRequest(msg)) => assert!(msg.contains("at most 8 facets"), "{msg}"),
            other => panic!("expected a bad request, got {other:?}"),
        }
        assert_eq!(era_params(&nine[..8]).unwrap().facets.len(), 8);
        assert!(era_params(&pairs(&[("dynasty_titles", "many")])).is_ok());
        assert!(
            dynasty_thresholds(&era_params(&pairs(&[("dynasty_titles", "many")])).unwrap())
                .is_err()
        );
    }
}
