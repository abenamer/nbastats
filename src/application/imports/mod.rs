//! Use case: import a dataset. Parse (CSV adapter) → upsert games (GameStore port, idempotent)
//! → rebuild derived tables → record provenance (ImportLog port).

pub mod derived;

use std::collections::BTreeSet;
use std::io::Read;
use std::path::Path;

use sha2::{Digest, Sha256};
use tracing::{info, warn};

use crate::adapters::csv::{self, ParseError, ParsedGame};
use crate::domain::ids::SeasonYear;
use crate::domain::models::{EloSource, GameType};

use super::ports::{
    DatasetSource, DownloadError, GameStore, ImportLog, ImportStart, ImportStats, NewGame,
    StoreError,
};

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct ImportOutcome {
    pub import_id: i64,
    pub format: String,
    pub rows_read: i64,
    pub rows_rejected: i64,
    pub games_inserted: i64,
    pub games_updated: i64,
    pub games_unchanged: i64,
    pub seasons_touched: Vec<SeasonYear>,
    pub skipped_duplicate: bool,
    pub notes: Vec<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum ImportError {
    #[error("cannot read file: {0}")]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Parse(#[from] ParseError),
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error(transparent)]
    Download(#[from] DownloadError),
}

/// Read a file, transparently decompressing gzip.
pub fn read_file(path: &Path) -> Result<Vec<u8>, ImportError> {
    let raw = std::fs::read(path)?;
    Ok(maybe_gunzip(raw)?)
}

/// Upper bound on a decompressed upload. A game log of 80 seasons is about 16 MB, so this
/// leaves a wide margin while stopping decompression bombs.
pub const MAX_DECOMPRESSED_BYTES: u64 = 512 * 1024 * 1024;

pub fn maybe_gunzip(raw: Vec<u8>) -> Result<Vec<u8>, std::io::Error> {
    if raw.len() > 2 && raw[0] == 0x1f && raw[1] == 0x8b {
        let mut out = Vec::new();
        flate2::read::GzDecoder::new(&raw[..])
            .take(MAX_DECOMPRESSED_BYTES + 1)
            .read_to_end(&mut out)?;
        if out.len() as u64 > MAX_DECOMPRESSED_BYTES {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("decompressed data exceeds {} bytes", MAX_DECOMPRESSED_BYTES),
            ));
        }
        Ok(out)
    } else {
        Ok(raw)
    }
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn gunzip_passes_plain_data_and_small_archives() {
        assert_eq!(maybe_gunzip(b"a,b\n".to_vec()).unwrap(), b"a,b\n");
        let mut enc = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        enc.write_all(b"date,season\n").unwrap();
        let gz = enc.finish().unwrap();
        assert_eq!(maybe_gunzip(gz).unwrap(), b"date,season\n");
    }

    #[test]
    fn gunzip_rejects_oversized_output() {
        // 600 MB of zeros compresses to under 1 MB; decompression must stop at the cap.
        let mut enc = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        let chunk = vec![0u8; 1 << 20];
        for _ in 0..600 {
            enc.write_all(&chunk).unwrap();
        }
        let gz = enc.finish().unwrap();
        assert!(gz.len() < 4 * 1024 * 1024);
        let err = maybe_gunzip(gz).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    }
}

/// Where a dataset came from, for provenance.
#[derive(Debug, Clone)]
pub struct Provenance<'a> {
    pub source: &'a str,
    pub source_url: Option<&'a str>,
    pub file_name: Option<&'a str>,
}

/// Import a CSV payload. `force` re-imports even if the identical file was imported before.
pub async fn import_bytes<S: GameStore + ImportLog>(
    store: &S,
    bytes: &[u8],
    provenance: Provenance<'_>,
    force: bool,
) -> Result<ImportOutcome, ImportError> {
    let sha = sha256_hex(bytes);
    if !force {
        if let Some(prev) = store.completed_import_with_sha(&sha).await? {
            info!(
                import_id = prev.id,
                "identical file already imported; skipping"
            );
            return Ok(ImportOutcome {
                import_id: prev.id,
                format: prev.format,
                rows_read: prev.rows_read,
                rows_rejected: prev.rows_rejected,
                games_inserted: 0,
                games_updated: 0,
                games_unchanged: prev.games_inserted + prev.games_updated + prev.games_unchanged,
                seasons_touched: vec![],
                skipped_duplicate: true,
                notes: vec!["identical file (same SHA-256) was already imported".into()],
            });
        }
    }
    let start = ImportStart {
        source: provenance.source,
        source_url: provenance.source_url,
        file_name: provenance.file_name,
        sha256: &sha,
        format: "unknown",
    };
    let report = match csv::parse(bytes) {
        Ok(r) => r,
        Err(e) => {
            let id = store.start_import(&start).await?;
            store.fail_import(id, &e.to_string()).await?;
            return Err(e.into());
        }
    };
    let import_id = store
        .start_import(&ImportStart {
            format: &report.format,
            ..start
        })
        .await?;
    match store_games(store, import_id, &report.games).await {
        Ok(mut outcome) => {
            outcome.import_id = import_id;
            outcome.format = report.format.clone();
            outcome.rows_read = report.rows_read;
            outcome.rows_rejected += report.rows_rejected;
            outcome.notes.extend(report.rejections.iter().cloned());
            let stats = ImportStats {
                rows_read: outcome.rows_read,
                games_inserted: outcome.games_inserted,
                games_updated: outcome.games_updated,
                games_unchanged: outcome.games_unchanged,
                rows_rejected: outcome.rows_rejected,
                min_season: outcome.seasons_touched.first().copied(),
                max_season: outcome.seasons_touched.last().copied(),
                notes: (!outcome.notes.is_empty()).then(|| outcome.notes.join("\n")),
            };
            store.finish_import(import_id, &stats).await?;
            Ok(outcome)
        }
        Err(e) => {
            store.fail_import(import_id, &e.to_string()).await?;
            Err(e.into())
        }
    }
}

/// Map parsed games to stored rows, upsert them, then rebuild the derived tables.
async fn store_games<S: GameStore>(
    store: &S,
    import_id: i64,
    games: &[ParsedGame],
) -> Result<ImportOutcome, StoreError> {
    let teams = store.team_code_map().await?;
    let mut unknown: BTreeSet<String> = BTreeSet::new();
    let mut rejected = 0i64;
    let mut seasons: BTreeSet<SeasonYear> = BTreeSet::new();
    let mut rows: Vec<NewGame> = Vec::with_capacity(games.len());
    for g in games {
        let Ok(season) = SeasonYear::try_new(g.season) else {
            rejected += 1;
            continue;
        };
        match (teams.get(&g.home), teams.get(&g.away)) {
            (Some(h), Some(a)) => {
                seasons.insert(season);
                rows.push(NewGame {
                    season,
                    date: g.date,
                    seq: g.seq,
                    home: h.id,
                    away: a.id,
                    neutral: g.neutral,
                    game_type: if g.playoff {
                        GameType::Playoff
                    } else {
                        GameType::Regular
                    },
                    home_score: g.home_score,
                    away_score: g.away_score,
                    home_elo_pre: g.home_elo_pre,
                    away_elo_pre: g.away_elo_pre,
                    home_elo_post: g.home_elo_post,
                    away_elo_post: g.away_elo_post,
                    home_win_prob: g.home_win_prob,
                    elo_source: EloSource::Imported,
                });
            }
            _ => {
                rejected += 1;
                unknown.extend(
                    [&g.home, &g.away]
                        .into_iter()
                        .filter(|c| !teams.contains_key(*c))
                        .cloned(),
                );
            }
        }
    }
    for &s in &seasons {
        store.ensure_season(s).await?;
    }
    let counts = store.upsert_games(import_id, &rows).await?;
    let mut notes = Vec::new();
    if !unknown.is_empty() {
        let list: Vec<String> = unknown.into_iter().collect();
        warn!(?list, "unknown team codes skipped");
        notes.push(format!("unknown team codes skipped: {}", list.join(", ")));
    }
    info!(
        inserted = counts.inserted,
        updated = counts.updated,
        unchanged = counts.unchanged,
        rejected,
        "games upserted"
    );
    store.sync_alignments().await?;
    let season_list: Vec<SeasonYear> = seasons.into_iter().collect();
    derived::rebuild_seasons(store, &season_list).await?;
    Ok(ImportOutcome {
        import_id,
        format: String::new(),
        rows_read: 0,
        rows_rejected: rejected,
        games_inserted: counts.inserted,
        games_updated: counts.updated,
        games_unchanged: counts.unchanged,
        seasons_touched: season_list,
        skipped_duplicate: false,
        notes,
    })
}

/// Import a local file (optionally gzip-compressed).
pub async fn import_path<S: GameStore + ImportLog>(
    store: &S,
    path: &Path,
    source: &str,
    force: bool,
) -> Result<ImportOutcome, ImportError> {
    let bytes = read_file(path)?;
    let name = path.file_name().map(|n| n.to_string_lossy().to_string());
    import_bytes(
        store,
        &bytes,
        Provenance {
            source,
            source_url: None,
            file_name: name.as_deref(),
        },
        force,
    )
    .await
}

/// Download the configured source and import it. The downloaded file is kept in `data_dir`
/// for provenance.
pub async fn refresh_from_url<S: GameStore + ImportLog, D: DatasetSource>(
    store: &S,
    source: &D,
    url: &str,
    data_dir: &Path,
    force: bool,
) -> Result<ImportOutcome, ImportError> {
    info!(url, "downloading dataset");
    let bytes = source.download(url).await?;
    std::fs::create_dir_all(data_dir.join("downloads"))?;
    let stamp = chrono::Utc::now().format("%Y%m%dT%H%M%SZ");
    let file_name = format!("nba_elo_{stamp}.csv");
    std::fs::write(data_dir.join("downloads").join(&file_name), &bytes)?;
    import_bytes(
        store,
        &bytes,
        Provenance {
            source: "download",
            source_url: Some(url),
            file_name: Some(&file_name),
        },
        force,
    )
    .await
}
