//! Shared application state: configuration, the wired adapters, an in-memory season cache
//! and the job-runner coordination primitives. Construction happens in the composition root
//! (`main.rs`), which is the one place that names the concrete adapters.

use std::collections::HashMap;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, RwLock};

use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::adapters::balldontlie::BalldontlieSource;
use crate::adapters::dataset::HttpDatasetSource;
use crate::adapters::openrouter::OpenRouterModel;
use crate::adapters::postgres::PostgresStore;
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
    /// Held while a results sync runs (manual or scheduled); at most one at a time.
    pub sync_lock: Arc<tokio::sync::Mutex<()>>,
    sync_phase: std::sync::Mutex<SyncPhase>,
    pub started_at: chrono::DateTime<chrono::Utc>,
    /// Wakes the job runner when a job is queued.
    pub job_notify: Notify,
    /// Cancellation requested by a user for a running job.
    pub cancel_flags: std::sync::Mutex<HashMap<Uuid, Arc<AtomicBool>>>,
    /// Application-wide shutdown signal.
    pub shutdown: CancellationToken,
    seasons: RwLock<HashMap<SeasonYear, Arc<SeasonData>>>,
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
    ) -> Self {
        Self {
            config,
            store,
            source,
            results,
            sync_schedule,
            model,
            ask_slots: tokio::sync::Semaphore::new(ASK_SLOTS),
            sync_lock: Arc::new(tokio::sync::Mutex::new(())),
            sync_phase: std::sync::Mutex::new(SyncPhase::Idle),
            started_at: chrono::Utc::now(),
            job_notify: Notify::new(),
            cancel_flags: std::sync::Mutex::new(HashMap::new()),
            shutdown: CancellationToken::new(),
            seasons: RwLock::new(HashMap::new()),
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
        self.seasons
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .insert(season, data.clone());
        Ok(data)
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

    /// Drop cached data after an import.
    pub fn invalidate_cache(&self) {
        self.seasons
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .clear();
        *self.teams.write().unwrap_or_else(|e| e.into_inner()) = None;
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
