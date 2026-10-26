//! Return from absence over a range of seasons: the events of every season with the players'
//! names, and the cells that group them; see `domain::recovery`. Each season's events and
//! controls are computed on the first request that covers it and cached until the next
//! import or sync; a report over any range aggregates the cached seasons.

use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::sync::Arc;

use serde::Serialize;

use crate::domain::ids::{PlayerId, SeasonYear};
use crate::domain::recovery::{self, Cell, ControlSample, Parameters, ReturnEvent, SeasonReturns};

use super::boxscore;
use super::ports::{PlayerStore, StoreError};
use super::state::{App, SeasonLoadError};

/// The most seasons one report covers: every season with box scores fits.
pub const MAX_SEASONS: usize = 80;
/// Seasons a report covers when none are asked for.
pub const DEFAULT_SEASONS: usize = 10;

/// One event with the player's name and his team's code.
#[derive(Debug, Clone, Serialize)]
pub struct NamedEvent {
    #[serde(flatten)]
    pub event: ReturnEvent,
    pub name: String,
    pub team_code: String,
}

/// A season of the range the report could not read.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SkippedSeason {
    pub season: SeasonYear,
    pub reason: String,
}

/// A season that was read, with its counts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SeasonCount {
    pub season: SeasonYear,
    pub team_games: u32,
    pub player_teams: u32,
    pub events: u32,
    pub controls: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReturnReport {
    pub from: SeasonYear,
    pub to: SeasonYear,
    pub parameters: Parameters,
    pub seasons: Vec<SeasonCount>,
    pub skipped: Vec<SkippedSeason>,
    pub events: Vec<NamedEvent>,
    /// Control samples over the range: every clean forty-game stretch of a regular player.
    pub controls: u32,
    pub cells: Vec<Cell>,
    /// Why the report is empty when it is.
    pub note: Option<String>,
}

/// One season's events and controls, from the cache or computed. A season whose box
/// scores do not support BPM comes back with no team games and a note (and is cached as
/// such); a season without games is `NoGames`.
async fn season(app: &App, season: SeasonYear) -> Result<Arc<SeasonReturns>, SeasonLoadError> {
    if let Some(r) = app
        .return_seasons
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .get(&season)
        .cloned()
    {
        return Ok(r);
    }
    let generation = app.cache_generation.load(Ordering::SeqCst);
    let data = app.season_data(season).await?;
    let table = boxscore::table(app, season).await?;
    let returns = if table.bpm_available {
        let lines = app.store.season_box_lines(season).await?;
        // Thousands of lines walked per player: off the request thread.
        tokio::task::spawn_blocking(move || {
            recovery::season_returns(season, &data.games, &lines, &table)
        })
        .await
        .map_err(|e| SeasonLoadError::Store(StoreError::Query(e.to_string())))?
    } else {
        SeasonReturns {
            season,
            team_games: 0,
            player_teams: 0,
            events: vec![],
            controls: vec![],
            note: Some(
                table
                    .note
                    .clone()
                    .unwrap_or_else(|| "the box scores do not support BPM".to_string()),
            ),
        }
    };
    let returns = Arc::new(returns);
    let mut cache = app
        .return_seasons
        .write()
        .unwrap_or_else(|e| e.into_inner());
    if app.cache_generation.load(Ordering::SeqCst) == generation {
        cache.insert(season, returns.clone());
    }
    Ok(returns)
}

/// The report over `from..=to` (validated by the caller: in order and within
/// [`MAX_SEASONS`]). A season without games or without box scores that support BPM is
/// skipped with its reason; a store failure is an error.
pub async fn report(
    app: &App,
    from: SeasonYear,
    to: SeasonYear,
) -> Result<ReturnReport, StoreError> {
    let mut seasons = Vec::new();
    let mut skipped = Vec::new();
    let mut events: Vec<ReturnEvent> = Vec::new();
    let mut controls: Vec<ControlSample> = Vec::new();
    for year in SeasonYear::range(from, to) {
        let returns = match season(app, year).await {
            Ok(r) => r,
            Err(SeasonLoadError::NoGames(_)) => {
                skipped.push(SkippedSeason {
                    season: year,
                    reason: "no games stored".to_string(),
                });
                continue;
            }
            Err(SeasonLoadError::Store(e)) => return Err(e),
        };
        if returns.team_games == 0 {
            skipped.push(SkippedSeason {
                season: year,
                reason: returns
                    .note
                    .clone()
                    .unwrap_or_else(|| "no box-score lines".to_string()),
            });
            continue;
        }
        seasons.push(SeasonCount {
            season: year,
            team_games: returns.team_games,
            player_teams: returns.player_teams,
            events: returns.events.len() as u32,
            controls: returns.controls.len() as u32,
        });
        events.extend(returns.events.iter().cloned());
        controls.extend(returns.controls.iter().cloned());
    }
    let cells = recovery::cells(&events, &controls);
    let mut ids: Vec<PlayerId> = events.iter().map(|e| e.player_id).collect();
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
    let named = events
        .into_iter()
        .map(|event| NamedEvent {
            name: players
                .get(&event.player_id)
                .map(|p| {
                    format!("{} {}", p.first_name, p.last_name)
                        .trim()
                        .to_string()
                })
                .unwrap_or_else(|| event.player_id.to_string()),
            team_code: teams
                .get(&event.team_id)
                .map(|t| t.code.clone())
                .unwrap_or_default(),
            event,
        })
        .collect::<Vec<_>>();
    let note = if named.is_empty() {
        Some(if seasons.is_empty() {
            "No season of the range has box scores that support Box Plus/Minus: load them on the Data page.".to_string()
        } else {
            "No regular player of these seasons missed five team games in a row and came back with twenty team games before the run and ten after the return.".to_string()
        })
    } else {
        None
    };
    Ok(ReturnReport {
        from,
        to,
        parameters: recovery::parameters(),
        seasons,
        skipped,
        events: named,
        controls: controls.len() as u32,
        cells,
        note,
    })
}
