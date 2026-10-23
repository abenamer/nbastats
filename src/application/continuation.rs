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
use crate::domain::continuation::{rate_games, ContinuationError, ResultGame};
use crate::domain::ids::{SeasonYear, TeamId};
use crate::domain::models::{EloSource, Game, GameType, ResultSource};

use super::recompute;

use super::imports::{self, derived, ImportOutcome};
use super::ports::{
    FetchProgressSink, FetchWindow, GameStore, ImportLog, ImportStart, ImportStats, NewGame,
    ResultRecord, ResultsSource, SourceError, StoreError,
};

#[derive(Debug, thiserror::Error)]
pub enum ContinuationUseCaseError {
    #[error("season {0} has results from the archive; a projection or a removal is only for seasons without archive games")]
    SeasonHasArchiveResults(SeasonYear),
    #[error("season {0} has no games; the previous season must exist to continue from")]
    NoPreviousSeason(SeasonYear),
    #[error("unknown team codes: {0}")]
    UnknownTeams(String),
    #[error("no games for season {0}: the source has nothing for it yet, so nothing was stored")]
    NoGames(SeasonYear),
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

async fn guard_not_archived<S: GameStore>(
    store: &S,
    season: SeasonYear,
) -> Result<(), ContinuationUseCaseError> {
    let existing = store.season_games(season).await?;
    if existing
        .iter()
        .any(|g| g.result_source == ResultSource::Archive)
    {
        return Err(ContinuationUseCaseError::SeasonHasArchiveResults(season));
    }
    Ok(())
}

/// Start ratings of `season` for `members`: the previous season's franchise end ratings,
/// reverted; a franchise without one takes the archive's first rating of the season when
/// `archive` has it, else the new-franchise rating. Without a previous season and without
/// archive games there is nothing to chain from.
async fn start_for<S: GameStore>(
    store: &S,
    season: SeasonYear,
    archive: &[Game],
    members: impl IntoIterator<Item = TeamId>,
) -> Result<HashMap<TeamId, f64>, ContinuationUseCaseError> {
    let previous = season
        .previous()
        .ok_or(ContinuationUseCaseError::OutOfRange(season.end_year() - 1))?;
    let prev_games = store.season_games(previous).await?;
    if prev_games.is_empty() && archive.is_empty() {
        return Err(ContinuationUseCaseError::NoPreviousSeason(previous));
    }
    let teams = store.team_map().await?;
    Ok(recompute::start_ratings(
        &prev_games,
        archive,
        members,
        &teams,
    ))
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

/// The incoming games the archive does not have. An incoming game that matches an archive
/// game (same date, same two teams) is dropped: the archive result stands. Returns the games
/// kept and the number dropped.
fn not_in_archive(archive: &[Game], incoming: Vec<ResultGame>) -> (Vec<ResultGame>, usize) {
    let keys: HashSet<(NaiveDate, TeamId, TeamId)> = archive
        .iter()
        .map(|g| game_key(g.game_date, g.home_team_id, g.away_team_id))
        .collect();
    let mut kept = Vec::with_capacity(incoming.len());
    let mut dropped = 0usize;
    for g in incoming {
        if keys.contains(&game_key(g.date, g.home, g.away)) {
            dropped += 1;
        } else {
            kept.push(g);
        }
    }
    (kept, dropped)
}

fn archive_of(games: &[Game]) -> Vec<Game> {
    games
        .iter()
        .filter(|g| g.result_source == ResultSource::Archive)
        .cloned()
        .collect()
}

/// Every team with a game in the archive rows or the incoming games.
fn members_of(archive: &[Game], incoming: &[ResultGame]) -> Vec<TeamId> {
    recompute::teams_in(archive)
        .chain(incoming.iter().flat_map(|g| [g.home, g.away]))
        .collect()
}

/// Where computed games came from, for the import log.
#[derive(Debug, Clone, Copy)]
struct Origin<'a> {
    source: &'a str,
    source_url: Option<&'a str>,
    file_name: Option<&'a str>,
    format: &'a str,
    /// Where the results came from. Archive rows of the season keep their own source.
    result_source: ResultSource,
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

/// Rate the season's archive games and `incoming` as one chain from `start`, then store the
/// result: the archive rows keep their identity and get the new ratings; the incoming games
/// replace the season's results and projection rows. One transaction.
async fn store_rated<S: GameStore + ImportLog>(
    store: &S,
    season: SeasonYear,
    archive: &[Game],
    incoming: Vec<ResultGame>,
    start: &HashMap<TeamId, f64>,
    origin: Origin<'_>,
    sha: &str,
) -> Result<ImportOutcome, ContinuationUseCaseError> {
    let Origin {
        source,
        source_url,
        file_name,
        format,
        result_source,
    } = origin;
    let mut games: Vec<ResultGame> = archive.iter().map(recompute::as_result_game).collect();
    games.extend(incoming);
    let rated = rate_games(start, &games)?;
    let (ratings, new_games) = recompute::ratings_of(archive, &rated);
    let import_id = store
        .start_import(&ImportStart {
            source,
            source_url,
            file_name,
            sha256: sha,
            format,
        })
        .await?;
    let rows: Vec<NewGame> = new_games
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
            result_source,
            reference: None,
        })
        .collect();
    let result: Result<ImportOutcome, StoreError> = async {
        store.ensure_season(season).await?;
        // Every rating of the season is computed again from the start ratings, because the
        // chain runs through every game. Archive rows stay (their ratings update); results and
        // projection rows are replaced.
        let counts = store
            .replace_season_games(season, import_id, &rows, &ratings)
            .await?;
        store.sync_alignments().await?;
        derived::rebuild_seasons(store, &[season]).await?;
        Ok(ImportOutcome {
            import_id,
            format: format.to_string(),
            rows_read: games.len() as i64,
            rows_rejected: 0,
            games_inserted: counts.inserted,
            games_updated: counts.updated,
            games_unchanged: counts.unchanged,
            seasons_touched: vec![season],
            skipped_duplicate: false,
            notes: vec![format!("Elo computed by the application from {} starting ratings ({} games with results, {} scheduled)",
                start.len(), games.iter().filter(|g| g.home_score.is_some()).count(), games.iter().filter(|g| g.home_score.is_none()).count())],
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
    let incoming = resolve_games(&teams, &records)?;
    let archive = archive_of(&store.season_games(season).await?);
    let (incoming, dropped) = not_in_archive(&archive, incoming);
    if incoming.is_empty() {
        return Err(ContinuationUseCaseError::NoGames(season));
    }
    let start = start_for(store, season, &archive, members_of(&archive, &incoming)).await?;
    let sha = imports::sha256_hex(bytes);
    let origin = Origin {
        source: "results",
        source_url: None,
        file_name,
        format: "results",
        result_source: ResultSource::Results,
    };
    let mut outcome = store_rated(store, season, &archive, incoming, &start, origin, &sha).await?;
    outcome.rows_read = report.rows_read;
    outcome.rows_rejected = report.rows_rejected;
    outcome.notes.extend(report.rejections);
    note_archive(&mut outcome, &archive, dropped, season);
    Ok(outcome)
}

fn note_archive(outcome: &mut ImportOutcome, archive: &[Game], dropped: usize, season: SeasonYear) {
    if archive.is_empty() {
        return;
    }
    outcome.notes.push(format!(
        "season {season} keeps its {} archive games ({dropped} incoming games were already in the archive); every rating of the season is computed again",
        archive.len()
    ));
    if let Some(next) = season.next() {
        outcome.notes.push(format!(
            "season {next} starts from these final ratings: sync it again to update its chain"
        ));
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

/// Pull `season` from a results source (BALLDONTLIE) and compute its Elo. The first sync of
/// a season fetches the whole season, schedule included, because the season forecast
/// simulates the games that are not played yet. Once the store holds a played game of the
/// season, a sync fetches only the days from the last played game to `today` and keeps the
/// stored games outside that window. Every call recomputes the ratings of the season from
/// the merged games, so a daily sync keeps the ratings current.
pub async fn sync_results<S: GameStore + ImportLog, R: ResultsSource>(
    store: &S,
    source: &R,
    season: SeasonYear,
    today: NaiveDate,
    progress: &dyn FetchProgressSink,
) -> Result<ImportOutcome, ContinuationUseCaseError> {
    let stored = store.season_games(season).await?;
    let window = fetch_window(&stored, today);
    let fetch = source.season_results(season, window, progress).await?;
    info!(
        season = %season,
        games = fetch.records.len(),
        requests = fetch.requests,
        source = source.name(),
        from = ?window.from,
        to = ?window.to,
        "fetched season results"
    );
    let teams = store.team_code_map().await?;
    let fetched = resolve_games(&teams, &fetch.records)?;
    // Results and projection rows merge with the fetch inside the window; archive rows stand.
    let archive = archive_of(&stored);
    let kept_stored = stored
        .iter()
        .filter(|g| g.result_source != ResultSource::Archive)
        .map(recompute::as_result_game)
        .collect();
    let merged = merge_games(kept_stored, fetched, window);
    let (incoming, dropped) = not_in_archive(&archive, merged);
    if incoming.is_empty() {
        return Err(ContinuationUseCaseError::NoGames(season));
    }
    let start = start_for(store, season, &archive, members_of(&archive, &incoming)).await?;
    let sha = imports::sha256_hex(format!("{incoming:?}").as_bytes());
    let origin = Origin {
        source: source.name(),
        source_url: Some(&fetch.source_url),
        file_name: None,
        format: "results",
        result_source: ResultSource::Results,
    };
    let mut outcome = store_rated(store, season, &archive, incoming, &start, origin, &sha).await?;
    outcome.notes.push(match (window.from, window.to) {
        (Some(from), Some(to)) => format!(
            "{} games fetched with {} requests for {from} to {to}; earlier games kept from the store",
            fetch.records.len(),
            fetch.requests
        ),
        _ => format!(
            "{} games fetched with {} requests",
            fetch.records.len(),
            fetch.requests
        ),
    });
    note_archive(&mut outcome, &archive, dropped, season);
    Ok(outcome)
}

/// The dates to ask the source for. Without a played game in the store the whole season is
/// fetched (the first sync, a projection, or a season that has not started). Otherwise the
/// window runs from the last played game (fetched again, so a late final is caught) to
/// `today`.
fn fetch_window(stored: &[Game], today: NaiveDate) -> FetchWindow {
    match stored
        .iter()
        .filter(|g| g.is_played())
        .map(|g| g.game_date)
        .max()
    {
        Some(last) => FetchWindow::between(last.min(today), today),
        None => FetchWindow::all(),
    }
}

/// The fetched games plus the stored games they do not replace. Inside the window the
/// source is the authority: a stored game without a result that the source no longer lists
/// on that date (a postponement) is dropped. Outside the window the store is kept as it is.
fn merge_games(
    stored: Vec<ResultGame>,
    fetched: Vec<ResultGame>,
    window: FetchWindow,
) -> Vec<ResultGame> {
    let fetched_keys: HashSet<(NaiveDate, TeamId, TeamId)> = fetched
        .iter()
        .map(|g| game_key(g.date, g.home, g.away))
        .collect();
    let mut games = fetched;
    games.extend(stored.into_iter().filter(|g| {
        !fetched_keys.contains(&game_key(g.date, g.home, g.away))
            && (!window.contains(g.date) || g.home_score.is_some())
    }));
    games.sort_by_key(|g| (g.date, g.home, g.away));
    games
}

/// Remove a season that holds only computed games (a projection, a sync of a season that
/// has not started, or an empty season row). Seasons with imported games are refused.
pub async fn remove_season<S: GameStore>(
    store: &S,
    season: SeasonYear,
) -> Result<u64, ContinuationUseCaseError> {
    guard_not_archived(store, season).await?;
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
    guard_not_archived(store, season).await?;
    let previous = season
        .previous()
        .ok_or(ContinuationUseCaseError::OutOfRange(season.end_year() - 1))?;
    let prev_games = store.season_games(previous).await?;
    if prev_games.is_empty() {
        return Err(ContinuationUseCaseError::NoPreviousSeason(previous));
    }
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
    let start = start_for(store, season, &[], members_of(&[], &schedule)).await?;
    let sha = imports::sha256_hex(format!("projection:{season}:{}", prev_games.len()).as_bytes());
    let origin = Origin {
        source: "projection",
        source_url: None,
        file_name: None,
        format: "projection",
        result_source: ResultSource::Projection,
    };
    store_rated(store, season, &[], schedule, &start, origin, &sha).await
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

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    fn game(date: &str, home: i32, away: i32, score: Option<(i32, i32)>) -> Game {
        Game {
            id: 0,
            season: SeasonYear::of(2027),
            game_date: d(date),
            seq: 0,
            home_team_id: TeamId::new(home),
            away_team_id: TeamId::new(away),
            neutral: false,
            game_type: GameType::Regular,
            home_score: score.map(|s| s.0),
            away_score: score.map(|s| s.1),
            home_elo_pre: 1500.0,
            away_elo_pre: 1500.0,
            home_elo_post: 1500.0,
            away_elo_post: 1500.0,
            home_win_prob: None,
            elo_source: EloSource::Computed,
            result_source: ResultSource::Results,
            ref_home_elo_pre: None,
            ref_away_elo_pre: None,
            ref_home_elo_post: None,
            ref_away_elo_post: None,
        }
    }

    fn result(date: &str, home: i32, away: i32, score: Option<(i32, i32)>) -> ResultGame {
        recompute::as_result_game(&game(date, home, away, score))
    }

    #[test]
    fn archive_games_stand_and_their_duplicates_are_dropped() {
        let mut archived = game("2026-10-21", 1, 2, Some((100, 90)));
        archived.result_source = ResultSource::Archive;
        let incoming = vec![
            result("2026-10-21", 2, 1, Some((90, 100))), // the archive game, teams swapped
            result("2026-10-23", 3, 4, Some((99, 98))),
        ];
        let (kept, dropped) = not_in_archive(&[archived.clone()], incoming);
        assert_eq!(dropped, 1);
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].home.as_i32(), 3);
        assert_eq!(
            members_of(&[archived], &kept)
                .iter()
                .map(|t| t.as_i32())
                .collect::<Vec<_>>(),
            [1, 2, 3, 4]
        );
        // Without archive games everything incoming is kept.
        assert_eq!(
            not_in_archive(&[], vec![result("2026-10-23", 3, 4, None)])
                .0
                .len(),
            1
        );
    }

    #[test]
    fn archive_rows_keep_their_identity_when_rerated() {
        // Two archive rows of a same-day rematch at the same venue (seq 0 and 1) and one
        // incoming game: the archive rows map back to their ids in stored order, the
        // incoming game becomes a new row.
        let mut first = game("2026-10-21", 1, 2, Some((100, 90)));
        first.id = 10;
        first.result_source = ResultSource::Archive;
        let mut second = first.clone();
        second.id = 11;
        second.seq = 1;
        second.home_score = Some(80);
        let archive = [first, second];
        let mut games: Vec<ResultGame> = archive.iter().map(recompute::as_result_game).collect();
        games.push(result("2026-10-23", 3, 4, Some((99, 98))));
        let start: HashMap<TeamId, f64> = (1..=4).map(|t| (TeamId::new(t), 1500.0)).collect();
        let rated = rate_games(&start, &games).unwrap();
        let (ratings, new_games) = recompute::ratings_of(&archive, &rated);
        assert_eq!(ratings.iter().map(|r| r.id).collect::<Vec<_>>(), [10, 11]);
        // The second row's pre-game rating is the first row's post-game rating.
        assert!((ratings[1].home_elo_pre - ratings[0].home_elo_post).abs() < 1e-9);
        assert_eq!(new_games.len(), 1);
        assert_eq!(new_games[0].game.home.as_i32(), 3);
    }

    #[test]
    fn the_whole_season_is_fetched_until_a_game_is_played() {
        let today = d("2026-10-23");
        assert_eq!(fetch_window(&[], today), FetchWindow::all());
        let scheduled = [
            game("2026-10-21", 1, 2, None),
            game("2026-10-22", 3, 4, None),
        ];
        assert_eq!(fetch_window(&scheduled, today), FetchWindow::all());
        let played = [
            game("2026-10-21", 1, 2, Some((100, 90))),
            game("2026-10-22", 3, 4, Some((99, 98))),
            game("2026-10-24", 1, 3, None),
        ];
        assert_eq!(
            fetch_window(&played, today),
            FetchWindow::between(d("2026-10-22"), today)
        );
        // A played game dated after today (a clock behind the results) does not invert the window.
        let future = [game("2026-10-25", 1, 2, Some((1, 0)))];
        assert_eq!(
            fetch_window(&future, today),
            FetchWindow::between(today, today)
        );
    }

    #[test]
    fn merge_keeps_the_store_outside_the_window_and_the_source_inside_it() {
        let window = FetchWindow::between(d("2026-10-22"), d("2026-10-23"));
        let stored = vec![
            result("2026-10-21", 1, 2, Some((100, 90))), // before the window: kept
            result("2026-10-22", 3, 4, None),            // in the window, now final: replaced
            result("2026-10-22", 5, 6, None),            // in the window, postponed: dropped
            result("2026-10-23", 1, 3, Some((80, 70))),  // in the window, played, not listed: kept
            result("2026-10-30", 2, 4, None),            // after the window: kept
        ];
        let fetched = vec![
            result("2026-10-22", 3, 4, Some((99, 98))),
            result("2026-10-23", 2, 5, None),
        ];
        let merged = merge_games(stored, fetched, window);
        let keys: Vec<(String, i32, i32, Option<i32>)> = merged
            .iter()
            .map(|g| {
                (
                    g.date.to_string(),
                    g.home.as_i32(),
                    g.away.as_i32(),
                    g.home_score,
                )
            })
            .collect();
        assert_eq!(
            keys,
            vec![
                ("2026-10-21".to_string(), 1, 2, Some(100)),
                ("2026-10-22".to_string(), 3, 4, Some(99)),
                ("2026-10-23".to_string(), 1, 3, Some(80)),
                ("2026-10-23".to_string(), 2, 5, None),
                ("2026-10-30".to_string(), 2, 4, None),
            ]
        );
        // A full fetch replaces everything the store had.
        let all = merge_games(
            vec![result("2026-10-21", 1, 2, None)],
            vec![result("2026-10-21", 2, 1, None)],
            FetchWindow::all(),
        );
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].home.as_i32(), 2);
    }
}
