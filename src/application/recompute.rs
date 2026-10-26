//! Compute first: the application rates every stored game itself, season after season,
//! from the first season in the store. The archive's ratings stay in the reference columns
//! for validation; the application's ratings are what every page and engine uses.
//!
//! `recompute_all` runs after an archive import and once at start when rows still carry
//! imported ratings (an installation from before compute first). It writes the ratings of
//! every season in one transaction, so a failure leaves the store as it was. The
//! continuation use cases share `ratings_of`, which maps rated games back to stored rows.

use std::collections::HashMap;

use tracing::info;

use crate::domain::continuation::{
    rate_games, season_start, ContinuationError, RatedGame, ResultGame,
};
use crate::domain::ids::{FranchiseId, SeasonYear, TeamId};
use crate::domain::models::{EloSource, Game, Team};

use super::imports::derived;
use super::ports::{GameRating, GameStore, StoreError};

#[derive(Debug, thiserror::Error)]
pub enum RecomputeError {
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error(transparent)]
    Rating(#[from] ContinuationError),
}

/// What a recompute wrote.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct RecomputeOutcome {
    pub seasons: usize,
    pub games: usize,
    pub updated: u64,
}

pub fn as_result_game(g: &Game) -> ResultGame {
    ResultGame {
        date: g.game_date,
        home: g.home_team_id,
        away: g.away_team_id,
        neutral: g.neutral,
        game_type: g.game_type,
        home_score: g.home_score,
        away_score: g.away_score,
    }
}

/// The end rating of every franchise after `games` (the rating after its team's last played
/// game), for the next season's start.
pub fn franchise_end_ratings(
    games: &[Game],
    teams: &HashMap<TeamId, Team>,
) -> HashMap<FranchiseId, f64> {
    let mut end = HashMap::new();
    for g in games.iter().filter(|g| g.is_played()) {
        for (team, elo) in [
            (g.home_team_id, g.home_elo_post),
            (g.away_team_id, g.away_elo_post),
        ] {
            if let Some(t) = teams.get(&team) {
                end.insert(t.franchise_id, elo);
            }
        }
    }
    end
}

/// The start ratings of a season for `members`: the reverted franchise end ratings of the
/// stored `previous` season, with the archive's first rating in `seed_games` (date order)
/// as the seed for a franchise without one.
pub fn start_ratings(
    previous: &[Game],
    seed_games: &[Game],
    members: impl IntoIterator<Item = TeamId>,
    teams: &HashMap<TeamId, Team>,
) -> HashMap<TeamId, f64> {
    let previous_end = franchise_end_ratings(previous, teams);
    let mut seed: HashMap<TeamId, f64> = HashMap::new();
    let mut ordered: Vec<&Game> = seed_games.iter().collect();
    ordered.sort_by_key(|g| (g.game_date, g.seq, g.id));
    for g in ordered {
        for (team, pre) in [
            (g.home_team_id, g.ref_home_elo_pre),
            (g.away_team_id, g.ref_away_elo_pre),
        ] {
            if let (Some(pre), false) = (pre, seed.contains_key(&team)) {
                seed.insert(team, pre);
            }
        }
    }
    let mut members: Vec<(TeamId, FranchiseId)> = members
        .into_iter()
        .filter_map(|t| teams.get(&t).map(|team| (t, team.franchise_id)))
        .collect();
    members.sort_unstable();
    members.dedup();
    season_start(&members, &previous_end, &seed)
}

/// Every team with a game in `games`.
pub fn teams_in(games: &[Game]) -> impl Iterator<Item = TeamId> + '_ {
    games.iter().flat_map(|g| [g.home_team_id, g.away_team_id])
}

/// Map rated games back to stored rows. `rate_games` sorts by (date, home, away) and is
/// stable, so rows that share a key (a same-day rematch at the same venue) are matched in
/// their stored order. Returns the ratings of the matched rows and the rated games that
/// have no stored row.
pub fn ratings_of<'a>(
    stored: &[Game],
    rated: &'a [RatedGame],
) -> (Vec<GameRating>, Vec<&'a RatedGame>) {
    let mut ids: HashMap<(chrono::NaiveDate, TeamId, TeamId), Vec<i64>> = HashMap::new();
    for g in stored {
        ids.entry((g.game_date, g.home_team_id, g.away_team_id))
            .or_default()
            .push(g.id);
    }
    let mut ratings = Vec::with_capacity(stored.len());
    let mut unmatched = Vec::new();
    for r in rated {
        let key = (r.game.date, r.game.home, r.game.away);
        match ids
            .get_mut(&key)
            .and_then(|v| (!v.is_empty()).then(|| v.remove(0)))
        {
            Some(id) => ratings.push(GameRating {
                id,
                home_elo_pre: r.home_elo_pre,
                away_elo_pre: r.away_elo_pre,
                home_elo_post: r.home_elo_post,
                away_elo_post: r.away_elo_post,
                home_win_prob: Some(r.home_win_prob),
            }),
            None => unmatched.push(r),
        }
    }
    (ratings, unmatched)
}

/// The ratings of every game of a season, rated from `start`.
pub fn season_ratings(
    games: &[Game],
    start: &HashMap<TeamId, f64>,
) -> Result<Vec<GameRating>, ContinuationError> {
    let input: Vec<ResultGame> = games.iter().map(as_result_game).collect();
    let rated = rate_games(start, &input)?;
    Ok(ratings_of(games, &rated).0)
}

/// Write `ratings` into the in-memory `games` (by id), as the store would.
fn apply_ratings(games: &mut [Game], ratings: &[GameRating]) {
    let by_id: HashMap<i64, &GameRating> = ratings.iter().map(|r| (r.id, r)).collect();
    for g in games {
        if let Some(r) = by_id.get(&g.id) {
            g.home_elo_pre = r.home_elo_pre;
            g.away_elo_pre = r.away_elo_pre;
            g.home_elo_post = r.home_elo_post;
            g.away_elo_post = r.away_elo_post;
            g.home_win_prob = r.home_win_prob;
            g.elo_source = EloSource::Computed;
        }
    }
}

/// Rate every season in the store in order, chaining the franchise end ratings from one
/// season to the next; write every rating in one transaction, then rebuild the derived
/// tables of every season.
pub async fn recompute_all<S: GameStore>(store: &S) -> Result<RecomputeOutcome, RecomputeError> {
    let teams = store.team_map().await?;
    let seasons: Vec<SeasonYear> = store
        .seasons()
        .await?
        .into_iter()
        .map(|s| s.season)
        .collect();
    let mut outcome = RecomputeOutcome::default();
    let mut ratings: Vec<GameRating> = Vec::new();
    let mut previous: Vec<Game> = Vec::new();
    for season in &seasons {
        let mut games = store.season_games(*season).await?;
        if games.is_empty() {
            continue;
        }
        let start = start_ratings(&previous, &games, teams_in(&games), &teams);
        let season_ratings = season_ratings(&games, &start)?;
        // The next season chains from the ratings just computed.
        apply_ratings(&mut games, &season_ratings);
        info!(%season, games = games.len(), "season rated");
        ratings.extend(season_ratings);
        outcome.seasons += 1;
        outcome.games += games.len();
        previous = games;
    }
    outcome.updated = store.update_ratings(&ratings).await?;
    derived::rebuild_seasons(store, &seasons).await?;
    info!(
        seasons = outcome.seasons,
        games = outcome.games,
        updated = outcome.updated,
        "ratings recomputed from the first season"
    );
    Ok(outcome)
}
