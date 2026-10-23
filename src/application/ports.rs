//! Ports: the interfaces the application requires from the outside world (REAS-ARCH-002).
//!
//! Each port is a small, named trait at a genuine boundary. The PostgreSQL adapter
//! (`adapters::postgres::PostgresStore`) implements every store port; the dataset adapter
//! (`adapters::dataset::HttpDatasetSource`) implements `DatasetSource`. Tests can substitute
//! in-memory fakes. Traits use `async fn`; callers are generic or use the concrete adapters,
//! so the `Send` bounds are checked where the futures are spawned.

use std::collections::HashMap;

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::domain::ids::{FranchiseId, SeasonYear, TeamId};
use crate::domain::models::{
    ChampionshipView, DataImport, EloSource, Franchise, Game, GameType, JobEvent, JobKind,
    JobStatus, PlayoffResult, ResultSource, Season, SimulationJob, Team, TeamSeason,
    TeamSeasonView,
};
use crate::domain::records::{ChampionRoad, GameSwing, Matchup, MatchupOrder, TeamSeasonStat};

/// Error from any store port. Adapters convert their own errors into these variants.
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("store unavailable: {0}")]
    Unavailable(String),
    #[error("store query failed: {0}")]
    Query(String),
    #[error("stored data is invalid: {0}")]
    InvalidData(String),
}

pub type StoreResult<T> = Result<T, StoreError>;

// ---------------------------------------------------------------------------------------
// Game log (read side)
// ---------------------------------------------------------------------------------------

/// One Elo observation of a franchise (after a game).
#[derive(Debug, Clone, Serialize)]
pub struct EloPoint {
    pub season: SeasonYear,
    pub game_date: NaiveDate,
    pub code: String,
    pub game_type: GameType,
    pub elo_post: f64,
}

#[allow(async_fn_in_trait)]
pub trait GameLog: Send + Sync {
    async fn teams(&self) -> StoreResult<Vec<Team>>;
    async fn franchises(&self, active_only: bool) -> StoreResult<Vec<Franchise>>;
    async fn franchise_by_key(&self, key: &str) -> StoreResult<Option<Franchise>>;
    async fn seasons(&self) -> StoreResult<Vec<Season>>;
    async fn latest_season(&self) -> StoreResult<Option<SeasonYear>>;
    async fn game_count(&self) -> StoreResult<i64>;
    async fn season_games(&self, season: SeasonYear) -> StoreResult<Vec<Game>>;
    /// Games on or before `cutoff` (inclusive): the query-level leakage guard.
    async fn season_games_until(
        &self,
        season: SeasonYear,
        cutoff: NaiveDate,
    ) -> StoreResult<Vec<Game>>;
    async fn franchise_elo_history(
        &self,
        franchise: FranchiseId,
        from: SeasonYear,
        to: SeasonYear,
    ) -> StoreResult<Vec<EloPoint>>;

    async fn team_map(&self) -> StoreResult<HashMap<TeamId, Team>> {
        Ok(self.teams().await?.into_iter().map(|t| (t.id, t)).collect())
    }
    async fn team_code_map(&self) -> StoreResult<HashMap<String, Team>> {
        Ok(self
            .teams()
            .await?
            .into_iter()
            .map(|t| (t.code.clone(), t))
            .collect())
    }
}

// ---------------------------------------------------------------------------------------
// Game store (write side used by the import use case)
// ---------------------------------------------------------------------------------------

/// A parsed game ready to be stored.
#[derive(Debug, Clone, PartialEq)]
pub struct NewGame {
    pub season: SeasonYear,
    pub date: NaiveDate,
    pub seq: i16,
    pub home: TeamId,
    pub away: TeamId,
    pub neutral: bool,
    pub game_type: GameType,
    pub home_score: Option<i32>,
    pub away_score: Option<i32>,
    pub home_elo_pre: f64,
    pub away_elo_pre: f64,
    pub home_elo_post: f64,
    pub away_elo_post: f64,
    pub home_win_prob: Option<f64>,
    pub elo_source: EloSource,
    pub result_source: ResultSource,
    /// The archive's ratings (pre home, pre away, post home, post away) for an archive row.
    pub reference: Option<[f64; 4]>,
}

/// The application's ratings for one stored game, written back after a recompute.
#[derive(Debug, Clone, PartialEq)]
pub struct GameRating {
    pub id: i64,
    pub home_elo_pre: f64,
    pub away_elo_pre: f64,
    pub home_elo_post: f64,
    pub away_elo_post: f64,
    pub home_win_prob: Option<f64>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct UpsertCounts {
    pub inserted: i64,
    pub updated: i64,
    pub unchanged: i64,
}

#[derive(Debug, Clone)]
pub struct SeasonSummary {
    pub season: SeasonYear,
    pub team_count: i32,
    pub game_count: i32,
    pub playoff_game_count: i32,
    pub play_in_game_count: i32,
    pub cup_final_game_count: i32,
    pub computed_game_count: i32,
    pub archive_game_count: i32,
    pub scheduled_game_count: i32,
    pub regular_season_end: Option<NaiveDate>,
    pub playoffs_start: Option<NaiveDate>,
    pub playoffs_end: Option<NaiveDate>,
    pub complete: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChampionshipSource {
    /// Derived from the imported game records.
    Derived,
    /// From the curated reference table (no complete playoff record in the data).
    Curated,
}

impl ChampionshipSource {
    pub const fn as_str(self) -> &'static str {
        match self {
            ChampionshipSource::Derived => "derived",
            ChampionshipSource::Curated => "curated",
        }
    }
}

#[derive(Debug, Clone)]
pub struct NewChampionship {
    pub season: SeasonYear,
    pub champion: TeamId,
    pub champion_franchise: FranchiseId,
    pub runner_up: Option<TeamId>,
    pub source: ChampionshipSource,
}

#[allow(async_fn_in_trait)]
pub trait GameStore: GameLog {
    /// Insert or update franchises and teams from the static lineage table (idempotent).
    async fn sync_reference(&self) -> StoreResult<()>;
    /// Insert or update conference/division rows for every (season, team) with games.
    async fn sync_alignments(&self) -> StoreResult<()>;
    async fn ensure_season(&self, season: SeasonYear) -> StoreResult<()>;
    /// Upsert all rows in one transaction. Unchanged rows are neither inserted nor updated.
    /// A row from a results source or a projection never changes the result, type or
    /// reference ratings of an archive row; the application's ratings always update.
    async fn upsert_games(&self, import_id: i64, rows: &[NewGame]) -> StoreResult<UpsertCounts>;
    /// Write the application's ratings for stored games (by id) and mark them `computed`.
    async fn update_ratings(&self, ratings: &[GameRating]) -> StoreResult<u64>;
    /// Replace the games of a season that did not come from the archive with `rows` and
    /// write `ratings` for the season's archive games (by id), in one transaction, so a
    /// failure leaves the season as it was. Archive rows are never deleted; one counts as
    /// updated when its ratings changed, else as unchanged.
    async fn replace_season_games(
        &self,
        season: SeasonYear,
        import_id: i64,
        rows: &[NewGame],
        ratings: &[GameRating],
    ) -> StoreResult<UpsertCounts>;
    /// Whether any game still carries ratings copied from the archive (`imported`), which
    /// the application replaces with its own at start.
    async fn has_imported_ratings(&self) -> StoreResult<bool>;
    /// Delete a whole season: its games, team seasons, championship, forecasts and the
    /// season row, in one transaction. The caller must have checked that no imported game
    /// remains. Returns the number of games deleted.
    async fn delete_season(&self, season: SeasonYear) -> StoreResult<u64>;
    async fn set_game_types(&self, ids: &[i64], game_type: GameType) -> StoreResult<()>;
    async fn update_season_summary(&self, summary: &SeasonSummary) -> StoreResult<()>;
    async fn upsert_team_season(&self, ts: &TeamSeason) -> StoreResult<()>;
    async fn upsert_championship(&self, c: &NewChampionship) -> StoreResult<()>;
}

// ---------------------------------------------------------------------------------------
// Import log
// ---------------------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct ImportStart<'a> {
    pub source: &'a str,
    pub source_url: Option<&'a str>,
    pub file_name: Option<&'a str>,
    pub sha256: &'a str,
    pub format: &'a str,
}

#[derive(Debug, Clone, Default)]
pub struct ImportStats {
    pub rows_read: i64,
    pub games_inserted: i64,
    pub games_updated: i64,
    pub games_unchanged: i64,
    pub rows_rejected: i64,
    pub min_season: Option<SeasonYear>,
    pub max_season: Option<SeasonYear>,
    pub notes: Option<String>,
}

#[allow(async_fn_in_trait)]
pub trait ImportLog: Send + Sync {
    async fn start_import(&self, start: &ImportStart<'_>) -> StoreResult<i64>;
    async fn finish_import(&self, id: i64, stats: &ImportStats) -> StoreResult<()>;
    async fn fail_import(&self, id: i64, error: &str) -> StoreResult<()>;
    async fn imports(&self, limit: i64) -> StoreResult<Vec<DataImport>>;
    async fn completed_import_with_sha(&self, sha256: &str) -> StoreResult<Option<DataImport>>;
    /// Imports left `running` by a crashed process are marked failed at startup.
    async fn mark_stale_imports_failed(&self) -> StoreResult<u64>;
}

// ---------------------------------------------------------------------------------------
// Job store
// ---------------------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct NewJob<'a> {
    pub kind: JobKind,
    pub name: Option<&'a str>,
    pub params: &'a Value,
    pub seed: Option<i64>,
    pub app_version: &'a str,
    pub model_version: &'a str,
}

/// Terminal outcome of a running job. The store applies it only when the job is still
/// `running` (REAS-STATE-003: persisted transitions are conditional updates).
#[derive(Debug, Clone)]
pub enum JobOutcome {
    Completed(Value),
    Failed(String),
    Interrupted(String),
    Cancelled(String),
}

impl JobOutcome {
    pub const fn status(&self) -> JobStatus {
        match self {
            JobOutcome::Completed(_) => JobStatus::Completed,
            JobOutcome::Failed(_) => JobStatus::Failed,
            JobOutcome::Interrupted(_) => JobStatus::Interrupted,
            JobOutcome::Cancelled(_) => JobStatus::Cancelled,
        }
    }
}

#[allow(async_fn_in_trait)]
pub trait JobStore: Send + Sync {
    async fn create_job(&self, new: &NewJob<'_>) -> StoreResult<SimulationJob>;
    async fn job(&self, id: Uuid) -> StoreResult<Option<SimulationJob>>;
    async fn jobs(&self, kind: Option<JobKind>, limit: i64) -> StoreResult<Vec<SimulationJob>>;
    /// Named, completed jobs of a kind.
    async fn scenarios(&self, kind: JobKind) -> StoreResult<Vec<SimulationJob>>;
    /// Atomically claim the oldest queued job (`queued -> running`).
    async fn claim_next_job(&self) -> StoreResult<Option<SimulationJob>>;
    async fn set_job_progress(&self, id: Uuid, progress: f64) -> StoreResult<()>;
    /// Record a terminal outcome; returns false when the job was not `running`.
    async fn finish_job(&self, id: Uuid, outcome: &JobOutcome) -> StoreResult<bool>;
    /// Apply a lifecycle event with a conditional update; returns the new status when applied.
    async fn apply_job_event(&self, id: Uuid, event: JobEvent) -> StoreResult<Option<JobStatus>>;
    /// Re-queue a failed, interrupted or cancelled job, clearing its result and progress.
    async fn retry_job(&self, id: Uuid) -> StoreResult<bool>;
    async fn rename_job(&self, id: Uuid, name: Option<&str>) -> StoreResult<bool>;
    /// Delete a job that is not running.
    async fn delete_job(&self, id: Uuid) -> StoreResult<bool>;
    /// Jobs left `running` by a previous process are marked interrupted at startup.
    async fn mark_running_jobs_interrupted(&self) -> StoreResult<u64>;
    async fn job_counts(&self) -> StoreResult<Vec<(JobStatus, i64)>>;
}

// ---------------------------------------------------------------------------------------
// Analysis store (derived tables and forecasts)
// ---------------------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct NewForecast<'a> {
    pub job_id: Uuid,
    pub model: &'a str,
    pub season: SeasonYear,
    pub team: TeamId,
    pub seed: Option<i32>,
    /// [p_second_round, p_conf_finals, p_finals, p_champion]
    pub probs: [f64; 4],
    pub actual_result: PlayoffResult,
    pub actual_champion: bool,
    pub cutoff: NaiveDate,
}

#[derive(Debug, Clone, Serialize)]
pub struct StoreHealth {
    pub ok: bool,
    pub latency_ms: f64,
    pub server_version: String,
    pub database_size: String,
    pub pool_size: u32,
    pub pool_idle: usize,
    pub migrations_applied: i64,
    pub error: Option<String>,
}

#[allow(async_fn_in_trait)]
pub trait AnalysisStore: Send + Sync {
    async fn team_seasons(&self, season: SeasonYear) -> StoreResult<Vec<TeamSeasonView>>;
    async fn franchise_seasons(
        &self,
        franchise: FranchiseId,
        from: SeasonYear,
        to: SeasonYear,
    ) -> StoreResult<Vec<TeamSeasonView>>;
    async fn championships(
        &self,
        from: SeasonYear,
        to: SeasonYear,
    ) -> StoreResult<Vec<ChampionshipView>>;
    /// Title counts per franchise key within a season range.
    async fn title_counts(
        &self,
        from: SeasonYear,
        to: SeasonYear,
    ) -> StoreResult<HashMap<String, i64>>;
    /// Seasons with a complete, reconstructable postseason.
    async fn complete_playoff_seasons(
        &self,
        from: SeasonYear,
        to: SeasonYear,
    ) -> StoreResult<Vec<SeasonYear>>;
    async fn insert_forecast(&self, forecast: &NewForecast<'_>) -> StoreResult<()>;
    /// Every team-season with its rating extremes, for the Elo records.
    async fn team_season_stats(&self) -> StoreResult<Vec<TeamSeasonStat>>;
    /// The `limit` games with the largest rating gain for the winner.
    async fn biggest_swings(&self, limit: i64) -> StoreResult<Vec<GameSwing>>;
    /// Played games ranked by the two teams' combined pre-game rating: the `limit` strongest
    /// or weakest, in that order.
    async fn matchups(&self, order: MatchupOrder, limit: i64) -> StoreResult<Vec<Matchup>>;
    /// The weakest played game of every season, once for NBA/BAA games and once for ABA
    /// games (a game belongs to the league of its teams); in season order.
    async fn weakest_matchup_per_season(&self) -> StoreResult<Vec<Matchup>>;
    /// Every champion (from the championships table, curated ones included) with the mean
    /// rating of its opponents before each regular-season and playoff game.
    async fn champion_roads(&self) -> StoreResult<Vec<ChampionRoad>>;
    /// Every (season, team code) with at least one game, played or scheduled: the league's
    /// membership for the season, which the played games alone understate early in a season.
    async fn season_rosters(&self) -> StoreResult<Vec<(SeasonYear, String)>>;
    async fn table_counts(&self) -> StoreResult<Vec<(String, i64)>>;
    async fn health(&self) -> StoreHealth;
}

// ---------------------------------------------------------------------------------------
// Dataset source
// ---------------------------------------------------------------------------------------

#[derive(Debug, thiserror::Error)]
pub enum DownloadError {
    #[error("download failed: {0}")]
    Transport(String),
    #[error("downloaded file is too small to be a dataset ({0} bytes)")]
    TooSmall(usize),
    #[error("downloaded file exceeds the size limit ({0} bytes)")]
    TooLarge(usize),
}

#[allow(async_fn_in_trait)]
pub trait DatasetSource: Send + Sync {
    async fn download(&self, url: &str) -> Result<Vec<u8>, DownloadError>;
}

// ---------------------------------------------------------------------------------------
// Results source (game results and schedule of a season without Elo data)
// ---------------------------------------------------------------------------------------

/// One game as a results source reports it: team codes in the dataset's convention (BRK,
/// CHO, PHO, ...), scores only when the game is final.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResultRecord {
    pub date: NaiveDate,
    pub home_code: String,
    pub away_code: String,
    pub neutral: bool,
    pub playoff: bool,
    pub home_score: Option<i32>,
    pub away_score: Option<i32>,
}

/// The dates a fetch asks the source for. `all` is the whole season, schedule included;
/// a bounded window is the daily refresh: from the last played game to today.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FetchWindow {
    /// First date, inclusive. `None` is the start of the season.
    pub from: Option<NaiveDate>,
    /// Last date, inclusive. `None` is the end of the season.
    pub to: Option<NaiveDate>,
}

impl FetchWindow {
    pub const fn all() -> Self {
        Self {
            from: None,
            to: None,
        }
    }

    pub const fn between(from: NaiveDate, to: NaiveDate) -> Self {
        Self {
            from: Some(from),
            to: Some(to),
        }
    }

    pub fn is_all(&self) -> bool {
        self.from.is_none() && self.to.is_none()
    }

    pub fn contains(&self, date: NaiveDate) -> bool {
        self.from.is_none_or(|f| f <= date) && self.to.is_none_or(|t| date <= t)
    }
}

/// Everything a results source returns for one season, with provenance for the import log.
#[derive(Debug, Clone, Default)]
pub struct ResultsFetch {
    pub records: Vec<ResultRecord>,
    /// Where the data came from, for the import log.
    pub source_url: String,
    /// Number of HTTP requests the fetch needed.
    pub requests: u32,
}

#[derive(Debug, thiserror::Error)]
pub enum SourceError {
    #[error("results source is not configured: {0}")]
    NotConfigured(String),
    #[error("results source rejected the request ({status}): {message}")]
    Rejected { status: u16, message: String },
    #[error("results source rate limit still exceeded after {0} retries")]
    RateLimited(u32),
    #[error("results source transport error: {0}")]
    Transport(String),
    #[error("results source returned an unexpected payload: {0}")]
    InvalidPayload(String),
}

/// Progress of one fetch: reported after every request and before every wait.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FetchProgress {
    pub requests: u32,
    pub games: usize,
    /// Time until the next request may start, when the source is waiting.
    pub waiting: Option<std::time::Duration>,
    /// The last wait was imposed by the source (HTTP 429), not by pacing.
    pub rate_limited: bool,
}

/// Receives fetch progress. Any `Fn(FetchProgress)` is a sink; `&|_| {}` ignores it.
pub trait FetchProgressSink: Sync {
    fn report(&self, progress: FetchProgress);
}

impl<F: Fn(FetchProgress) + Sync> FetchProgressSink for F {
    fn report(&self, progress: FetchProgress) {
        self(progress)
    }
}

#[allow(async_fn_in_trait)]
pub trait ResultsSource: Send + Sync {
    /// Human-readable source name for the import log (for example `balldontlie`).
    fn name(&self) -> &'static str;
    /// Every game of `season` inside `window` the source knows: played games with scores,
    /// scheduled games without. Progress is reported to `progress` as pages arrive.
    async fn season_results(
        &self,
        season: SeasonYear,
        window: FetchWindow,
        progress: &dyn FetchProgressSink,
    ) -> Result<ResultsFetch, SourceError>;
}

// ---------------------------------------------------------------------------------------
// Language model (the question box)
// ---------------------------------------------------------------------------------------

/// A function the model may ask for. `parameters` is a JSON Schema object.
#[derive(Debug, Clone, Serialize)]
pub struct ToolSpec {
    pub name: &'static str,
    pub description: &'static str,
    pub parameters: Value,
}

/// A call the model asks for: the tool's name and its JSON arguments.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: Value,
}

/// One turn of the conversation sent to the model.
#[derive(Debug, Clone, PartialEq)]
pub enum ChatMessage {
    System(String),
    User(String),
    Assistant {
        content: Option<String>,
        tool_calls: Vec<ToolCall>,
    },
    /// The result of a tool call, as text (JSON) for the model.
    Tool {
        call_id: String,
        content: String,
    },
}

#[derive(Debug, Clone, Copy)]
pub struct ChatRequest<'a> {
    pub messages: &'a [ChatMessage],
    pub tools: &'a [ToolSpec],
    pub max_tokens: u32,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ChatReply {
    pub content: Option<String>,
    pub tool_calls: Vec<ToolCall>,
    pub input_tokens: u64,
    pub output_tokens: u64,
}

#[derive(Debug, thiserror::Error)]
pub enum ModelError {
    #[error("language model not configured: {0}")]
    NotConfigured(String),
    #[error("language model request failed: {0}")]
    Transport(String),
    #[error("language model answered with status {status}: {message}")]
    Rejected { status: u16, message: String },
    #[error("language model answer is invalid: {0}")]
    InvalidResponse(String),
}

/// A chat model with tool calling. The concrete adapter is `adapters::openrouter::OpenRouterModel`.
#[allow(async_fn_in_trait)]
pub trait LanguageModel: Send + Sync {
    /// The model identifier shown to the user.
    fn model_name(&self) -> &str;
    async fn chat(&self, request: ChatRequest<'_>) -> Result<ChatReply, ModelError>;
}

// ---------------------------------------------------------------------------------------
// Game reports (a link to the wire-service report of a played game)
// ---------------------------------------------------------------------------------------

/// A report found for a game, stored by date and team codes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GameReport {
    pub game_date: NaiveDate,
    pub home_code: String,
    pub away_code: String,
    pub url: String,
    pub title: Option<String>,
    pub source: String,
}

/// One date asked of the report source, with how many reports it gave.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReportLookup {
    pub game_date: NaiveDate,
    pub source: String,
    pub fetched_at: chrono::DateTime<chrono::Utc>,
    pub found: i32,
}

#[allow(async_fn_in_trait)]
pub trait ReportStore: Send + Sync {
    async fn reports_on(&self, dates: &[NaiveDate]) -> StoreResult<Vec<GameReport>>;
    async fn lookups_on(&self, dates: &[NaiveDate]) -> StoreResult<Vec<ReportLookup>>;
    /// Record one date's lookup and its reports in one transaction.
    async fn save_lookup(
        &self,
        date: NaiveDate,
        source: &str,
        reports: &[GameReport],
    ) -> StoreResult<()>;
}

/// A report the source lists for a date, with the teams as the source names them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReportCandidate {
    pub home_abbr: String,
    pub away_abbr: String,
    pub home_name: String,
    pub away_name: String,
    pub url: String,
    pub title: Option<String>,
}

/// What the source lists for a date: how many games, and the reports among them. A date
/// the source lists no game for is not treated as final (the feed may have been empty).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReportListing {
    pub games_listed: usize,
    pub reports: Vec<ReportCandidate>,
}

/// Where game reports come from. The concrete adapter is `adapters::espn::EspnReportSource`.
#[allow(async_fn_in_trait)]
pub trait ReportSource: Send + Sync {
    fn name(&self) -> &'static str;
    async fn reports_on(&self, date: NaiveDate) -> Result<ReportListing, SourceError>;
}
