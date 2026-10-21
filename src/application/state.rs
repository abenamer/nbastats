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
use crate::adapters::postgres::PostgresStore;
use crate::config::Config;
use crate::domain::ids::{SeasonYear, TeamId};
use crate::domain::models::{Game, Team};
use crate::domain::playoffs::reconstruct::{self, ReconstructError, Reconstruction};

use super::ports::{GameLog, StoreError};

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
    ) -> Self {
        Self {
            config,
            store,
            source,
            results,
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
