//! Shared application state: configuration, database pool, in-memory season cache and the
//! job-runner coordination primitives.

use std::collections::HashMap;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, RwLock};

use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::config::Config;
use crate::adapters::postgres::{self as db, Pool};
use crate::domain::models::{Game, Team};
use crate::domain::playoffs::reconstruct::{self, ReconstructError, Reconstruction};

/// Everything the engines need to know about one season, loaded once and shared.
pub struct SeasonData {
    pub season: i32,
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

pub struct App {
    pub config: Config,
    pub pool: Pool,
    pub started_at: chrono::DateTime<chrono::Utc>,
    /// Wakes the job runner when a job is queued.
    pub job_notify: Notify,
    /// Cancellation requested by a user for a running job.
    pub cancel_flags: std::sync::Mutex<HashMap<Uuid, Arc<AtomicBool>>>,
    /// Application-wide shutdown signal.
    pub shutdown: CancellationToken,
    seasons: RwLock<HashMap<i32, Arc<SeasonData>>>,
    teams: RwLock<Option<Arc<HashMap<i32, Team>>>>,
}

impl App {
    pub fn new(config: Config, pool: Pool) -> Self {
        Self {
            config,
            pool,
            started_at: chrono::Utc::now(),
            job_notify: Notify::new(),
            cancel_flags: std::sync::Mutex::new(HashMap::new()),
            shutdown: CancellationToken::new(),
            seasons: RwLock::new(HashMap::new()),
            teams: RwLock::new(None),
        }
    }

    pub async fn teams(&self) -> anyhow::Result<Arc<HashMap<i32, Team>>> {
        if let Some(t) = self.teams.read().expect("teams lock").clone() {
            return Ok(t);
        }
        let map = Arc::new(db::teams::team_map(&self.pool).await?);
        *self.teams.write().expect("teams lock") = Some(map.clone());
        Ok(map)
    }

    pub async fn season_data(&self, season: i32) -> anyhow::Result<Arc<SeasonData>> {
        if let Some(d) = self.seasons.read().expect("season lock").get(&season).cloned() {
            return Ok(d);
        }
        let games = db::games::season_games(&self.pool, season).await?;
        if games.is_empty() {
            anyhow::bail!("no games stored for season {season}");
        }
        let teams = self.teams().await?;
        let reconstruction = reconstruct::reconstruct(season, &games, &teams);
        let data = Arc::new(SeasonData { season, games, reconstruction });
        self.seasons.write().expect("season lock").insert(season, data.clone());
        Ok(data)
    }

    /// Drop cached data after an import.
    pub fn invalidate_cache(&self) {
        self.seasons.write().expect("season lock").clear();
        *self.teams.write().expect("teams lock") = None;
    }

    pub fn cancel_flag(&self, id: Uuid) -> Arc<AtomicBool> {
        self.cancel_flags.lock().expect("cancel lock").entry(id).or_default().clone()
    }

    pub fn clear_cancel_flag(&self, id: Uuid) {
        self.cancel_flags.lock().expect("cancel lock").remove(&id);
    }
}
