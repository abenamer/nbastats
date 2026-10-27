//! Shared application state: configuration, the wired adapters, an in-memory season cache
//! and the job-runner coordination primitives. Construction happens in the composition root
//! (`main.rs`), which is the one place that names the concrete adapters.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, RwLock};

use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::adapters::balldontlie::BalldontlieSource;
use crate::adapters::dataset::HttpDatasetSource;
use crate::adapters::espn::EspnReportSource;
use crate::adapters::openrouter::OpenRouterModel;
use crate::adapters::postgres::PostgresStore;
use crate::application::players::PlayerPhase;
use crate::application::results_sync::SyncPhase;
use crate::application::scheduler::SyncSchedule;
use crate::config::Config;
use crate::domain::ids::{SeasonYear, TeamId};
use crate::domain::models::{Game, Team};
use crate::domain::playoffs::reconstruct::{self, ReconstructError, Reconstruction};

use super::ports::{GameLog, StoreError};

/// Questions the question box answers at the same time.
pub const ASK_SLOTS: usize = 2;

/// Everything the engines need to know about one season, loaded once and shared.
pub struct SeasonData {
    pub season: SeasonYear,
    pub games: Vec<Game>,
    pub reconstruction: Result<Reconstruction, ReconstructError>,
}

impl SeasonData {
    /// Last game date before the playoffs: the information cutoff of a pre-playoff forecast.
    pub fn pre_playoff_cutoff(&self) -> Option<chrono::NaiveDate> {
        let first_playoff = self.games.iter().find(|g| g.is_playoff())?.game_date;
        Some(first_playoff - chrono::Duration::days(1))
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SeasonLoadError {
    #[error("no games stored for season {0}")]
    NoGames(SeasonYear),
    #[error(transparent)]
    Store(#[from] StoreError),
}

pub struct App {
    pub config: Config,
    /// The store adapter. Use cases access it through the port traits.
    pub store: PostgresStore,
    pub source: HttpDatasetSource,
    /// Results source for seasons beyond the dataset; `None` until an API key is configured.
    pub results: Option<BalldontlieSource>,
    /// Daily sync schedule; `None` when disabled or when there is no results source.
    pub sync_schedule: Option<SyncSchedule>,
    /// Language model for the question box; `None` until an OpenRouter key is configured.
    pub model: Option<OpenRouterModel>,
    /// Questions answered at the same time; the rest get status 429.
    pub ask_slots: tokio::sync::Semaphore,
    /// Game report source; `None` when the lookups are turned off.
    pub reports: Option<EspnReportSource>,
    /// One report lookup at a time; a request that finds it taken answers from the store.
    pub report_slot: tokio::sync::Semaphore,
    /// One backward reading of a title record at a time (seconds of exact brackets); the
    /// rest get status 429.
    pub strength_slot: tokio::sync::Semaphore,
    /// One return-from-absence report at a time (it may read every season's lines); the
    /// rest get status 429.
    pub research_slot: tokio::sync::Semaphore,
    /// Held while a results sync runs (manual or scheduled); at most one at a time.
    pub sync_lock: Arc<tokio::sync::Mutex<()>>,
    sync_phase: std::sync::Mutex<SyncPhase>,
    /// Held while a player sync runs (backfill, daily run or injury report); at most one.
    pub player_lock: Arc<tokio::sync::Mutex<()>>,
    player_phase: std::sync::Mutex<PlayerPhase>,
    pub started_at: chrono::DateTime<chrono::Utc>,
    /// Wakes the job runner when a job is queued.
    pub job_notify: Notify,
    /// Cancellation requested by a user for a running job.
    pub cancel_flags: std::sync::Mutex<HashMap<Uuid, Arc<AtomicBool>>>,
    /// Application-wide shutdown signal.
    pub shutdown: CancellationToken,
    seasons: RwLock<HashMap<SeasonYear, Arc<SeasonData>>>,
    /// The home-court bonus of each season's forecasts, fitted once per season.
    home_advantages: RwLock<HashMap<SeasonYear, crate::domain::elo::HomeAdvantage>>,
    /// The player worth of each season, computed once per season until the next import.
    pub(crate) worths: RwLock<HashMap<SeasonYear, Arc<crate::domain::worth::WorthTable>>>,
    /// The box-score worth of each season, likewise.
    pub(crate) box_tables: RwLock<HashMap<SeasonYear, Arc<crate::domain::bpm::BoxTable>>>,
    /// The return events and control samples of each season, likewise.
    pub(crate) return_seasons:
        RwLock<HashMap<SeasonYear, Arc<crate::domain::recovery::SeasonReturns>>>,
    /// The runs of missed games of each season, likewise.
    pub(crate) spell_runs: RwLock<HashMap<SeasonYear, Arc<crate::domain::spell::SeasonRuns>>>,
    /// The report lines of each season followed through its game logs, likewise.
    pub(crate) status_samples:
        RwLock<HashMap<SeasonYear, Arc<Vec<crate::domain::status::StatusSample>>>>,
    /// Counts the cache invalidations, so a load that began before one is not cached after it.
    pub(crate) cache_generation: AtomicU64,
    teams: RwLock<Option<Arc<HashMap<TeamId, Team>>>>,
}

impl App {
    pub fn new(
        config: Config,
        store: PostgresStore,
        source: HttpDatasetSource,
        results: Option<BalldontlieSource>,
        sync_schedule: Option<SyncSchedule>,
        model: Option<OpenRouterModel>,
        reports: Option<EspnReportSource>,
    ) -> Self {
        Self {
            config,
            store,
            source,
            results,
            sync_schedule,
            model,
            ask_slots: tokio::sync::Semaphore::new(ASK_SLOTS),
            reports,
            report_slot: tokio::sync::Semaphore::new(1),
            strength_slot: tokio::sync::Semaphore::new(1),
            research_slot: tokio::sync::Semaphore::new(1),
            sync_lock: Arc::new(tokio::sync::Mutex::new(())),
            sync_phase: std::sync::Mutex::new(SyncPhase::Idle),
            player_lock: Arc::new(tokio::sync::Mutex::new(())),
            player_phase: std::sync::Mutex::new(PlayerPhase::Idle),
            started_at: chrono::Utc::now(),
            job_notify: Notify::new(),
            cancel_flags: std::sync::Mutex::new(HashMap::new()),
            shutdown: CancellationToken::new(),
            seasons: RwLock::new(HashMap::new()),
            home_advantages: RwLock::new(HashMap::new()),
            worths: RwLock::new(HashMap::new()),
            box_tables: RwLock::new(HashMap::new()),
            return_seasons: RwLock::new(HashMap::new()),
            spell_runs: RwLock::new(HashMap::new()),
            status_samples: RwLock::new(HashMap::new()),
            cache_generation: AtomicU64::new(0),
            teams: RwLock::new(None),
        }
    }

    pub async fn teams(&self) -> Result<Arc<HashMap<TeamId, Team>>, StoreError> {
        if let Some(t) = self.teams.read().unwrap_or_else(|e| e.into_inner()).clone() {
            return Ok(t);
        }
        let map = Arc::new(self.store.team_map().await?);
        *self.teams.write().unwrap_or_else(|e| e.into_inner()) = Some(map.clone());
        Ok(map)
    }

    pub async fn season_data(
        &self,
        season: SeasonYear,
    ) -> Result<Arc<SeasonData>, SeasonLoadError> {
        if let Some(d) = self
            .seasons
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .get(&season)
            .cloned()
        {
            return Ok(d);
        }
        let generation = self.cache_generation.load(Ordering::SeqCst);
        let games = self.store.season_games(season).await?;
        if games.is_empty() {
            return Err(SeasonLoadError::NoGames(season));
        }
        let teams = self.teams().await?;
        let reconstruction = reconstruct::reconstruct(season, &games, &teams);
        let data = Arc::new(SeasonData {
            season,
            games,
            reconstruction,
        });
        let mut seasons = self.seasons.write().unwrap_or_else(|e| e.into_inner());
        if self.cache_generation.load(Ordering::SeqCst) == generation {
            seasons.insert(season, data.clone());
        }
        Ok(data)
    }

    /// The home-court bonus the forecasts of `season` use: the regular-season value fitted on
    /// the seasons before it (see `application::home_advantage`), cached until the next import.
    /// A season of the window without games contributes nothing; a store failure is returned
    /// and nothing is cached, so a bonus fitted on a partial window never stands.
    pub async fn home_advantage(
        &self,
        season: SeasonYear,
    ) -> Result<crate::domain::elo::HomeAdvantage, StoreError> {
        if let Some(h) = self
            .home_advantages
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .get(&season)
        {
            return Ok(h.clone());
        }
        let generation = self.cache_generation.load(Ordering::SeqCst);
        let mut per_season = Vec::new();
        for earlier in super::home_advantage::window(season) {
            let samples = match self.season_data(earlier).await {
                Ok(data) => super::home_advantage::samples(&data.games),
                Err(SeasonLoadError::NoGames(_)) => Vec::new(),
                Err(SeasonLoadError::Store(e)) => return Err(e),
            };
            per_season.push((earlier, samples));
        }
        // The fit is two passes over at most a few thousand samples per step, about a
        // millisecond in all, so it runs on the request's thread.
        let fitted = super::home_advantage::from_samples(&per_season);
        let mut cache = self
            .home_advantages
            .write()
            .unwrap_or_else(|e| e.into_inner());
        if self.cache_generation.load(Ordering::SeqCst) == generation {
            cache.insert(season, fitted.clone());
        }
        Ok(fitted)
    }

    /// Today's date in the configured sync time zone (`RESULTS_SYNC_TIMEZONE`, whether or
    /// not the daily sync is on): the last day a results sync asks the source for.
    pub fn today(&self) -> chrono::NaiveDate {
        let zone: chrono_tz::Tz = self
            .config
            .results_sync_timezone
            .trim()
            .parse()
            .unwrap_or(chrono_tz::America::New_York);
        chrono::Utc::now().with_timezone(&zone).date_naive()
    }

    pub fn sync_phase(&self) -> SyncPhase {
        self.sync_phase
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    pub fn set_sync_phase(&self, phase: SyncPhase) {
        *self.sync_phase.lock().unwrap_or_else(|e| e.into_inner()) = phase;
    }

    pub fn player_phase(&self) -> PlayerPhase {
        self.player_phase
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    pub fn set_player_phase(&self, phase: PlayerPhase) {
        *self.player_phase.lock().unwrap_or_else(|e| e.into_inner()) = phase;
    }

    /// Drop cached data after an import. The generation moves first, under the season lock,
    /// so a load that began before the call cannot put its stale data into either cache.
    pub fn invalidate_cache(&self) {
        let mut seasons = self.seasons.write().unwrap_or_else(|e| e.into_inner());
        self.cache_generation.fetch_add(1, Ordering::SeqCst);
        seasons.clear();
        drop(seasons);
        self.home_advantages
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .clear();
        self.worths
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .clear();
        self.box_tables
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .clear();
        self.return_seasons
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .clear();
        self.spell_runs
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .clear();
        self.status_samples
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .clear();
        *self.teams.write().unwrap_or_else(|e| e.into_inner()) = None;
    }

    /// Drop the caches read from the players, box scores and injury reports after a player
    /// sync: the worth tables, the box tables, the return seasons, the runs and the status
    /// samples. The games and teams stand. The generation moves first, as above.
    pub fn invalidate_player_caches(&self) {
        self.cache_generation.fetch_add(1, Ordering::SeqCst);
        self.worths
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .clear();
        self.box_tables
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .clear();
        self.return_seasons
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .clear();
        self.spell_runs
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .clear();
        self.status_samples
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .clear();
    }

    pub fn cancel_flag(&self, id: Uuid) -> Arc<AtomicBool> {
        self.cancel_flags
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .entry(id)
            .or_default()
            .clone()
    }

    pub fn clear_cancel_flag(&self, id: Uuid) {
        self.cancel_flags
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&id);
    }
}
