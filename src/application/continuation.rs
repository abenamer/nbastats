//! Use cases for seasons the Elo dataset does not cover.
//!
//! * `import_results`: rate a season's game results (or schedule) from the previous season's
//!   final ratings with FiveThirtyEight's formulas and store them as computed Elo.
//! * `sync_results`: the same, with the results pulled from a `ResultsSource` adapter
//!   (BALLDONTLIE) instead of an uploaded file.
//! * `project_season`: build a preseason projection of a season with no data at all: reverted
//!   ratings on a synthetic schedule that repeats the previous season's regular-season
//!   matchups. Nothing is invented about results; the schedule is marked as a projection.

use std::collections::{HashMap, HashSet};

use chrono::{Datelike, NaiveDate};
use tracing::info;

use crate::adapters::csv::{self, ResultRow};
use crate::domain::continuation::{
    rate_games, season_start_ratings, ContinuationError, ResultGame,
};
use crate::domain::ids::{SeasonYear, TeamId};
use crate::domain::models::{EloSource, Game, GameType};

use super::imports::{self, derived, ImportOutcome};
use super::ports::{
    FetchProgressSink, GameStore, ImportLog, ImportStart, ImportStats, NewGame, ResultRecord,
    ResultsSource, SourceError, StoreError,
};

#[derive(Debug, thiserror::Error)]
pub enum ContinuationUseCaseError {
    #[error("season {0} is already covered by imported Elo data; computed ratings are only for later seasons")]
    SeasonHasImportedElo(SeasonYear),
    #[error("season {0} has no games; the previous season must exist to continue from")]
    NoPreviousSeason(SeasonYear),
    #[error("unknown team codes: {0}")]
    UnknownTeams(String),
    #[error("no games for season {0}: the source has nothing for it yet, so nothing was stored")]
    NoGames(SeasonYear),
    #[error("season {season}: a game on {date} is not in the archive (matched by date and the two teams) but predates its last archive game ({last_imported}); the archive part of a season cannot be changed")]
    InsideImportedRange {
        season: SeasonYear,
        date: NaiveDate,
        last_imported: NaiveDate,
    },
    #[error("season {0} is outside the supported range")]
    OutOfRange(i32),
    #[error(transparent)]
    Parse(#[from] csv::ParseError),
    #[error(transparent)]
    Source(#[from] SourceError),
    #[error(transparent)]
    Rating(#[from] ContinuationError),
    #[error(transparent)]
    Store(#[from] StoreError),
}

/// Final rating of every team after the last game of `season` (playoffs included), which is
/// what FiveThirtyEight reverts toward the mean for the next season.
fn final_ratings(games: &[Game]) -> HashMap<TeamId, f64> {
    games
        .iter()
        .filter(|g| g.is_played())
        .fold(HashMap::new(), |mut m, g| {
            m.insert(g.home_team_id, g.home_elo_post);
            m.insert(g.away_team_id, g.away_elo_post);
            m
        })
}

async fn start_ratings<S: GameStore>(
    store: &S,
    season: SeasonYear,
) -> Result<HashMap<TeamId, f64>, ContinuationUseCaseError> {
    let previous = season
        .previous()
        .ok_or(ContinuationUseCaseError::OutOfRange(season.end_year() - 1))?;
    let prev_games = store.season_games(previous).await?;
    if prev_games.is_empty() {
        return Err(ContinuationUseCaseError::NoPreviousSeason(previous));
    }
    Ok(season_start_ratings(&final_ratings(&prev_games)))
}

async fn guard_not_imported<S: GameStore>(
    store: &S,
    season: SeasonYear,
) -> Result<(), ContinuationUseCaseError> {
    let existing = store.season_games(season).await?;
    if existing.iter().any(|g| g.elo_source == EloSource::Imported) {
        return Err(ContinuationUseCaseError::SeasonHasImportedElo(season));
    }
    Ok(())
}

/// Where the computed ratings of a season start.
enum Baseline {
    /// No imported game: reverted ratings from the previous season's final ratings.
    NewSeason(HashMap<TeamId, f64>),
    /// Imported games exist (the archive covers part of the season): ratings after the last
    /// imported game. Only games after `last_imported` that are not in the archive are added.
    Completion {
        last_imported: NaiveDate,
        ratings: HashMap<TeamId, f64>,
        imported_keys: HashSet<(NaiveDate, TeamId, TeamId)>,
    },
}

impl Baseline {
    fn ratings(&self) -> &HashMap<TeamId, f64> {
        match self {
            Baseline::NewSeason(r) | Baseline::Completion { ratings: r, .. } => r,
        }
    }
}

async fn baseline<S: GameStore>(
    store: &S,
    season: SeasonYear,
) -> Result<Baseline, ContinuationUseCaseError> {
    let existing = store.season_games(season).await?;
    let imported: Vec<&Game> = existing
        .iter()
        .filter(|g| g.elo_source == EloSource::Imported)
        .collect();
    let Some(last) = imported.iter().map(|g| g.game_date).max() else {
        return Ok(Baseline::NewSeason(start_ratings(store, season).await?));
    };
    let ratings = imported
        .iter()
        .filter(|g| g.is_played())
        .fold(HashMap::new(), |mut m, g| {
            m.insert(g.home_team_id, g.home_elo_post);
            m.insert(g.away_team_id, g.away_elo_post);
            m
        });
    Ok(Baseline::Completion {
        last_imported: last,
        ratings,
        imported_keys: imported
            .iter()
            .map(|g| game_key(g.game_date, g.home_team_id, g.away_team_id))
            .collect(),
    })
}

/// Identity of a game for matching a source against the archive: the date and the two teams
/// as an unordered pair. Sources disagree on which team is "home" at a neutral site (the NBA
/// Cup final), so orientation is not part of the key.
fn game_key(date: NaiveDate, a: TeamId, b: TeamId) -> (NaiveDate, TeamId, TeamId) {
    if a <= b {
        (date, a, b)
    } else {
        (date, b, a)
    }
}

/// Keep the games the baseline does not cover. For a completion, games already in the
/// archive are dropped and a game dated before the last archive game is an error, because
/// a computed game cannot sit inside the imported rating chain.
fn games_to_rate(
    games: Vec<ResultGame>,
    baseline: &Baseline,
    season: SeasonYear,
) -> Result<(Vec<ResultGame>, usize), ContinuationUseCaseError> {
    match baseline {
        Baseline::NewSeason(_) => Ok((games, 0)),
        Baseline::Completion {
            last_imported,
            imported_keys,
            ..
        } => {
            let mut kept = Vec::with_capacity(games.len());
            let mut skipped = 0usize;
            for g in games {
                if imported_keys.contains(&game_key(g.date, g.home, g.away)) {
                    skipped += 1;
                } else if g.date <= *last_imported {
                    return Err(ContinuationUseCaseError::InsideImportedRange {
                        season,
                        date: g.date,
                        last_imported: *last_imported,
                    });
                } else {
                    kept.push(g);
                }
            }
            Ok((kept, skipped))
        }
    }
}

/// Where computed games came from, for the import log.
#[derive(Debug, Clone, Copy)]
struct Origin<'a> {
    source: &'a str,
    source_url: Option<&'a str>,
    file_name: Option<&'a str>,
    format: &'a str,
}

/// Resolve team codes to identifiers. Unknown codes are collected and reported together.
fn resolve_games(
    teams: &HashMap<String, crate::domain::models::Team>,
    rows: &[ResultRecord],
) -> Result<Vec<ResultGame>, ContinuationUseCaseError> {
    let mut unknown: Vec<String> = Vec::new();
    let mut games = Vec::with_capacity(rows.len());
    for r in rows {
        match (teams.get(&r.home_code), teams.get(&r.away_code)) {
            (Some(h), Some(a)) => games.push(ResultGame {
                date: r.date,
                home: h.id,
                away: a.id,
                neutral: r.neutral,
                playoff: r.playoff,
                home_score: r.home_score,
                away_score: r.away_score,
            }),
            _ => {
                for c in [&r.home_code, &r.away_code] {
                    if !teams.contains_key(c) && !unknown.contains(c) {
                        unknown.push(c.clone());
                    }
                }
            }
        }
    }
    if unknown.is_empty() {
        Ok(games)
    } else {
        Err(ContinuationUseCaseError::UnknownTeams(unknown.join(", ")))
    }
}

async fn store_rated<S: GameStore + ImportLog>(
    store: &S,
    season: SeasonYear,
    games: Vec<ResultGame>,
    start: &HashMap<TeamId, f64>,
    origin: Origin<'_>,
    sha: &str,
) -> Result<ImportOutcome, ContinuationUseCaseError> {
    let Origin {
        source,
        source_url,
        file_name,
        format,
    } = origin;
    let rated = rate_games(start, &games)?;
    let import_id = store
        .start_import(&ImportStart {
            source,
            source_url,
            file_name,
            sha256: sha,
            format,
        })
        .await?;
    let rows: Vec<NewGame> = rated
        .iter()
        .map(|r| NewGame {
            season,
            date: r.game.date,
            seq: 0,
            home: r.game.home,
            away: r.game.away,
            neutral: r.game.neutral,
            game_type: if r.game.playoff {
                GameType::Playoff
            } else {
                GameType::Regular
            },
            home_score: r.game.home_score,
            away_score: r.game.away_score,
            home_elo_pre: r.home_elo_pre,
            away_elo_pre: r.away_elo_pre,
            home_elo_post: r.home_elo_post,
            away_elo_post: r.away_elo_post,
            home_win_prob: Some(r.home_win_prob),
            elo_source: EloSource::Computed,
        })
        .collect();
    let result: Result<ImportOutcome, StoreError> = async {
        store.ensure_season(season).await?;
        // Recompute from scratch: ratings chain through every game, so a partial update is wrong.
        store.delete_computed_games(season).await?;
        let counts = store.upsert_games(import_id, &rows).await?;
        store.sync_alignments().await?;
        derived::rebuild_seasons(store, &[season]).await?;
        Ok(ImportOutcome {
            import_id,
            format: format.to_string(),
            rows_read: rows.len() as i64,
            rows_rejected: 0,
            games_inserted: counts.inserted,
            games_updated: counts.updated,
            games_unchanged: counts.unchanged,
            seasons_touched: vec![season],
            skipped_duplicate: false,
            notes: vec![format!("Elo computed by the application from {} starting ratings ({} games with results, {} scheduled)",
                start.len(), rows.iter().filter(|r| r.home_score.is_some()).count(), rows.iter().filter(|r| r.home_score.is_none()).count())],
        })
    }
    .await;
    match result {
        Ok(outcome) => {
            store
                .finish_import(
                    import_id,
                    &ImportStats {
                        rows_read: outcome.rows_read,
                        games_inserted: outcome.games_inserted,
                        games_updated: outcome.games_updated,
                        games_unchanged: outcome.games_unchanged,
                        rows_rejected: 0,
                        min_season: Some(season),
                        max_season: Some(season),
                        notes: Some(outcome.notes.join("\n")),
                    },
                )
                .await?;
            info!(%season, games = outcome.rows_read, "computed Elo stored");
            Ok(outcome)
        }
        Err(e) => {
            store.fail_import(import_id, &e.to_string()).await?;
            Err(e.into())
        }
    }
}

/// Import game results (and scheduled games) for `season` and compute their Elo.
pub async fn import_results<S: GameStore + ImportLog>(
    store: &S,
    season: SeasonYear,
    bytes: &[u8],
    file_name: Option<&str>,
) -> Result<ImportOutcome, ContinuationUseCaseError> {
    let report = csv::parse_results(bytes)?;
    let teams = store.team_code_map().await?;
    let records: Vec<ResultRecord> = report.games.iter().map(ResultRecord::from).collect();
    if records.is_empty() {
        return Err(ContinuationUseCaseError::NoGames(season));
    }
    let games = resolve_games(&teams, &records)?;
    let base = baseline(store, season).await?;
    let (games, skipped) = games_to_rate(games, &base, season)?;
    let sha = imports::sha256_hex(bytes);
    let origin = Origin {
        source: "results",
        source_url: None,
        file_name,
        format: "results",
    };
    let mut outcome = store_rated(store, season, games, base.ratings(), origin, &sha).await?;
    outcome.rows_read = report.rows_read;
    outcome.rows_rejected = report.rows_rejected;
    outcome.notes.extend(report.rejections);
    note_completion(&mut outcome, &base, skipped, season);
    Ok(outcome)
}

fn note_completion(
    outcome: &mut ImportOutcome,
    base: &Baseline,
    skipped: usize,
    season: SeasonYear,
) {
    if let Baseline::Completion { last_imported, .. } = base {
        outcome.notes.push(format!(
            "season {season} keeps its {skipped} archive games (through {last_imported}); the new games chain from the archive ratings"
        ));
        if let Some(next) = season.next() {
            outcome.notes.push(format!(
                "season {next} starts from these final ratings: sync it again to update its chain"
            ));
        }
    }
}

impl From<&ResultRow> for ResultRecord {
    fn from(r: &ResultRow) -> Self {
        Self {
            date: r.date,
            home_code: r.home.clone(),
            away_code: r.away.clone(),
            neutral: r.neutral,
            playoff: r.playoff,
            home_score: r.home_score,
            away_score: r.away_score,
        }
    }
}

/// Pull `season` from a results source (BALLDONTLIE) and compute its Elo. Every call
/// replaces the computed games of that season, so a daily sync keeps the ratings current.
pub async fn sync_results<S: GameStore + ImportLog, R: ResultsSource>(
    store: &S,
    source: &R,
    season: SeasonYear,
    progress: &dyn FetchProgressSink,
) -> Result<ImportOutcome, ContinuationUseCaseError> {
    let fetch = source.season_results(season, progress).await?;
    info!(
        season = %season,
        games = fetch.records.len(),
        requests = fetch.requests,
        source = source.name(),
        "fetched season results"
    );
    if fetch.records.is_empty() {
        return Err(ContinuationUseCaseError::NoGames(season));
    }
    let teams = store.team_code_map().await?;
    let games = resolve_games(&teams, &fetch.records)?;
    let base = baseline(store, season).await?;
    let (games, skipped) = games_to_rate(games, &base, season)?;
    if games.is_empty() {
        return Err(ContinuationUseCaseError::NoGames(season));
    }
    let sha = imports::sha256_hex(format!("{:?}", fetch.records).as_bytes());
    let origin = Origin {
        source: source.name(),
        source_url: Some(&fetch.source_url),
        file_name: None,
        format: "results",
    };
    let mut outcome = store_rated(store, season, games, base.ratings(), origin, &sha).await?;
    outcome.notes.push(format!(
        "{} games fetched with {} requests",
        fetch.records.len(),
        fetch.requests
    ));
    note_completion(&mut outcome, &base, skipped, season);
    Ok(outcome)
}

/// Remove a season that holds only computed games (a projection, a sync of a season that
/// has not started, or an empty season row). Seasons with imported games are refused.
pub async fn remove_season<S: GameStore>(
    store: &S,
    season: SeasonYear,
) -> Result<u64, ContinuationUseCaseError> {
    guard_not_imported(store, season).await?;
    let deleted = store.delete_season(season).await?;
    info!(season = %season, games = deleted, "season removed");
    Ok(deleted)
}

/// Build a preseason projection for `season`: reverted ratings and a synthetic schedule that
/// repeats the previous season's regular-season matchups one year later.
pub async fn project_season<S: GameStore + ImportLog>(
    store: &S,
    season: SeasonYear,
) -> Result<ImportOutcome, ContinuationUseCaseError> {
    guard_not_imported(store, season).await?;
    let previous = season
        .previous()
        .ok_or(ContinuationUseCaseError::OutOfRange(season.end_year() - 1))?;
    let prev_games = store.season_games(previous).await?;
    if prev_games.is_empty() {
        return Err(ContinuationUseCaseError::NoPreviousSeason(previous));
    }
    let start = season_start_ratings(&final_ratings(&prev_games));
    let schedule: Vec<ResultGame> = prev_games
        .iter()
        .filter(|g| g.game_type == GameType::Regular)
        .map(|g| ResultGame {
            date: shift_one_year(g.game_date),
            home: g.home_team_id,
            away: g.away_team_id,
            neutral: g.neutral,
            playoff: false,
            home_score: None,
            away_score: None,
        })
        .collect();
    let sha = imports::sha256_hex(format!("projection:{season}:{}", prev_games.len()).as_bytes());
    let origin = Origin {
        source: "projection",
        source_url: None,
        file_name: None,
        format: "projection",
    };
    store_rated(store, season, schedule, &start, origin, &sha).await
}

fn shift_one_year(d: NaiveDate) -> NaiveDate {
    d.with_year(d.year() + 1)
        .unwrap_or_else(|| d + chrono::Duration::days(365))
}

/// Rows of the results layout as a CSV string (for documentation and tests).
pub fn results_example() -> &'static str {
    "date,home,away,home_score,away_score,neutral,playoff\n2025-10-21,OKC,HOU,125,124,0,0\n2025-10-22,LAL,GSW,,,0,0\n"
}

#[allow(dead_code)]
fn _results_row_type_is_used(_: ResultRow) {}
