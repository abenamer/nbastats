//! The draft prior: what each draft slot's rookies have been worth in their first season,
//! pooled over the stored seasons before the one priced; see `domain::rookie`.

use std::sync::atomic::Ordering;
use std::sync::Arc;

use serde::Serialize;

use crate::domain::bpm::BoxTable;
use crate::domain::ids::{PlayerId, SeasonYear};
use crate::domain::rookie::{self, DraftBracket, DraftPrior, DraftTable, RookieSample};

use super::boxscore;
use super::ports::PlayerStore;
use super::state::{App, SeasonLoadError};

/// The seasons before the one priced whose rookies are pooled. Each costs a box table on
/// a cold cache, so the window is short; six seasons give every bracket but the first
/// pick its minimum many times over.
pub const DRAFT_SEASONS: usize = 6;

/// The draft table with the seasons it rests on and the choices behind it, for the reader.
#[derive(Debug, Clone, Serialize)]
pub struct DraftSummary {
    #[serde(flatten)]
    pub table: DraftTable,
    pub seasons: Vec<SeasonYear>,
    pub min_players: usize,
    pub prior_minutes: f64,
}

/// A player's season in a box table, over every stint: his minutes, and his box worth
/// weighted by the minutes of each stint. None when he has no stint with minutes.
pub fn season_box(table: &BoxTable, player: PlayerId) -> Option<(f64, f64)> {
    let (minutes, weighted) = table
        .players
        .iter()
        .filter(|p| p.player_id == player)
        .fold((0.0, 0.0), |(m, w), p| {
            (m + p.minutes, w + p.minutes * p.worth_elo)
        });
    (minutes > 0.0).then_some((weighted / minutes, minutes))
}

/// The rookies of `season` (drafted the year before its first game) with their box worth
/// that season: one sample per player, over his stints.
async fn samples(app: &App, season: SeasonYear) -> Result<Vec<RookieSample>, SeasonLoadError> {
    let table = boxscore::table(app, season).await?;
    if !table.bpm_available {
        return Ok(vec![]);
    }
    let mut ids: Vec<PlayerId> = table.players.iter().map(|p| p.player_id).collect();
    ids.sort();
    ids.dedup();
    let players = app.store.players_by_id(&ids).await?;
    let draft_year = i32::from(season) - 1;
    Ok(players
        .iter()
        .filter(|p| p.draft_year == Some(draft_year))
        .filter_map(|p| {
            season_box(&table, p.id).map(|(worth, _)| RookieSample {
                bracket: DraftBracket::of(p.draft_round, p.draft_number),
                worth_elo: worth,
            })
        })
        .collect())
}

/// The draft table for pricing `season`: the rookies of the [`DRAFT_SEASONS`] seasons
/// before it, cached until the next import or sync. The table may hold no prior when too
/// few rookies are stored. On a cold cache this builds the box tables of those seasons,
/// one after another.
pub async fn draft_table(
    app: &App,
    season: SeasonYear,
) -> Result<Arc<DraftSummary>, SeasonLoadError> {
    if let Some(t) = app
        .draft_tables
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .get(&season)
        .cloned()
    {
        return Ok(t);
    }
    let generation = app.cache_generation.load(Ordering::SeqCst);
    let mut pooled = Vec::new();
    let mut seasons = Vec::new();
    let mut year = season;
    for _ in 0..DRAFT_SEASONS {
        let Some(previous) = year.previous() else {
            break;
        };
        year = previous;
        match samples(app, year).await {
            Ok(s) => {
                if !s.is_empty() {
                    seasons.push(year);
                    pooled.extend(s);
                }
            }
            Err(SeasonLoadError::NoGames(_)) => continue,
            Err(SeasonLoadError::Store(e)) => return Err(SeasonLoadError::Store(e)),
        }
    }
    seasons.reverse();
    let summary = Arc::new(DraftSummary {
        table: DraftTable::fit(&pooled),
        seasons,
        min_players: rookie::MIN_PLAYERS,
        prior_minutes: rookie::PRIOR_MINUTES,
    });
    let mut cache = app.draft_tables.write().unwrap_or_else(|e| e.into_inner());
    if app.cache_generation.load(Ordering::SeqCst) == generation {
        cache.insert(season, summary.clone());
    }
    Ok(summary)
}

impl DraftSummary {
    /// The prior for a bracket, or its nearest wider stand-in.
    pub fn prior_for(&self, bracket: DraftBracket) -> Option<&DraftPrior> {
        self.table.prior_for(bracket)
    }
}
