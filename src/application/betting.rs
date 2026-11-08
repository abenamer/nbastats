//! Use case: the betting portfolio over every season with a market record. The scored games
//! come from the track record of each season (`application::tracking::record`), so a game is
//! bet on exactly when the Model vs Market page scores it; `domain::betting` runs the pot.

use serde::Serialize;

use crate::domain::betting::{self, Portfolio, Settings};
use crate::domain::ids::SeasonYear;
use crate::domain::market::DEFAULT_MIN_RETURN;

use super::ports::MarketStore;
use super::state::{App, SeasonLoadError};
use super::tracking;

/// The portfolio with what it was run over.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BetsReport {
    pub portfolio: Portfolio,
    /// The seasons with a snapshot, first to last.
    pub seasons: Vec<SeasonYear>,
    /// Scored games over those seasons, and the snapshots waiting for a result or matching
    /// no stored game.
    pub scored: usize,
    pub pending: usize,
    pub unmatched: usize,
    /// The sportsbooks whose lines the scored games carry.
    pub providers: Vec<String>,
    pub note: Option<String>,
}

/// The portfolio under `settings` over every season with a snapshot.
pub async fn portfolio(app: &App, settings: Settings) -> Result<BetsReport, SeasonLoadError> {
    let seasons = app.store.snapshot_seasons().await?;
    let mut scored = Vec::new();
    let (mut pending, mut unmatched) = (0, 0);
    for &season in &seasons {
        let record = tracking::record(app, season, DEFAULT_MIN_RETURN, usize::MAX).await?;
        pending += record.pending;
        unmatched += record.unmatched;
        scored.extend(record.games);
    }
    let mut providers: Vec<String> = scored.iter().map(|g| g.snapshot.provider.clone()).collect();
    providers.sort_unstable();
    providers.dedup();
    let note = scored.is_empty().then(|| {
        "No scored game is stored: the record fills as games the sportsbook priced are played, and from the sportsbook's history with the market backfill.".to_string()
    });
    Ok(BetsReport {
        portfolio: betting::portfolio(&scored, &settings),
        seasons,
        scored: scored.len(),
        pending,
        unmatched,
        providers,
        note,
    })
}
