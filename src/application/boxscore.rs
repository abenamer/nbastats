//! Box-score worth per season: Box Plus/Minus 2.0, VORP and PER from the stored box scores,
//! with the players' names; see `domain::bpm`.

use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::sync::Arc;

use serde::Serialize;

use crate::domain::bpm::{self, BoxTable, PlayerBox};
use crate::domain::ids::{PlayerId, SeasonYear};

use super::ports::{PlayerStore, StoreError};
use super::state::{App, SeasonLoadError};

/// One player's season with one team, with his name for the page.
#[derive(Debug, Clone, Serialize)]
pub struct NamedBox {
    #[serde(flatten)]
    pub row: PlayerBox,
    pub name: String,
    pub team_code: String,
    /// The position the source lists, which the estimate leans on for low minutes.
    pub listed_position: Option<String>,
}

/// The box-score worth table of a season, cached until the next import or sync.
pub async fn table(app: &App, season: SeasonYear) -> Result<Arc<BoxTable>, SeasonLoadError> {
    if let Some(t) = app
        .box_tables
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .get(&season)
        .cloned()
    {
        return Ok(t);
    }
    let generation = app.cache_generation.load(Ordering::SeqCst);
    let data = app.season_data(season).await?;
    let lines = app.store.season_box_lines(season).await?;
    let mut ids: Vec<PlayerId> = lines.iter().map(|l| l.player_id).collect();
    ids.sort_unstable();
    ids.dedup();
    let listed: HashMap<PlayerId, f64> = app
        .store
        .players_by_id(&ids)
        .await?
        .into_iter()
        .filter_map(|p| {
            p.position
                .as_deref()
                .and_then(bpm::listed_position)
                .map(|v| (p.id, v))
        })
        .collect();
    // A few thousand lines and a fixed-point iteration: off the request thread.
    let computed = tokio::task::spawn_blocking(move || {
        Arc::new(bpm::table(season, &data.games, &lines, &listed))
    })
    .await
    .map_err(|e| SeasonLoadError::Store(StoreError::Query(e.to_string())))?;
    let mut cache = app.box_tables.write().unwrap_or_else(|e| e.into_inner());
    if app.cache_generation.load(Ordering::SeqCst) == generation {
        cache.insert(season, computed.clone());
    }
    Ok(computed)
}

/// The table with names and team codes on every row.
pub async fn named_table(
    app: &App,
    season: SeasonYear,
) -> Result<(Arc<BoxTable>, Vec<NamedBox>), SeasonLoadError> {
    let t = table(app, season).await?;
    let mut ids: Vec<PlayerId> = t.players.iter().map(|p| p.player_id).collect();
    ids.sort_unstable();
    ids.dedup();
    let players: HashMap<PlayerId, _> = app
        .store
        .players_by_id(&ids)
        .await?
        .into_iter()
        .map(|p| (p.id, p))
        .collect();
    let teams = app.teams().await?;
    let rows = t
        .players
        .iter()
        .map(|row| {
            let player = players.get(&row.player_id);
            NamedBox {
                row: row.clone(),
                name: player
                    .map(|p| {
                        format!("{} {}", p.first_name, p.last_name)
                            .trim()
                            .to_string()
                    })
                    .unwrap_or_else(|| row.player_id.to_string()),
                team_code: teams
                    .get(&row.team_id)
                    .map(|t| t.code.clone())
                    .unwrap_or_default(),
                listed_position: player.and_then(|p| p.position.clone()),
            }
        })
        .collect();
    Ok((t, rows))
}
