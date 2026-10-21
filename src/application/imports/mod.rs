//! Dataset import pipeline: parse → upsert games (idempotent) → rebuild derived tables.


pub mod derived;

use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::path::Path;

use sha2::{Digest, Sha256};
use tracing::{info, warn};


use crate::adapters::csv;
use crate::adapters::postgres::{self as db, imports::ImportStats, Pool};
use crate::domain::models::GameType;
use crate::domain::reference;

#[derive(Debug, Clone, serde::Serialize)]
pub struct ImportOutcome {
    pub import_id: i64,
    pub format: String,
    pub rows_read: i64,
    pub rows_rejected: i64,
    pub games_inserted: i64,
    pub games_updated: i64,
    pub games_unchanged: i64,
    pub seasons_touched: Vec<i32>,
    pub skipped_duplicate: bool,
    pub notes: Vec<String>,
}

/// Read a file, transparently decompressing gzip.
pub fn read_file(path: &Path) -> anyhow::Result<Vec<u8>> {
    let raw = std::fs::read(path)?;
    maybe_gunzip(raw)
}

pub fn maybe_gunzip(raw: Vec<u8>) -> anyhow::Result<Vec<u8>> {
    if raw.len() > 2 && raw[0] == 0x1f && raw[1] == 0x8b {
        let mut out = Vec::new();
        flate2::read::GzDecoder::new(&raw[..]).read_to_end(&mut out)?;
        Ok(out)
    } else {
        Ok(raw)
    }
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

/// Import a CSV payload. `force` re-imports even if the identical file was imported before.
pub async fn import_bytes(
    pool: &Pool,
    bytes: &[u8],
    source: &str,
    source_url: Option<&str>,
    file_name: Option<&str>,
    force: bool,
) -> anyhow::Result<ImportOutcome> {
    let sha = sha256_hex(bytes);
    if !force {
        if let Some(prev) = db::imports::completed_with_sha(pool, &sha).await? {
            info!(import_id = prev.id, "identical file already imported; skipping");
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
    let report = match csv::parse(bytes) {
        Ok(r) => r,
        Err(e) => {
            let id = db::imports::start(pool, source, source_url, file_name, &sha, "unknown").await?;
            db::imports::fail(pool, id, &e.to_string()).await?;
            return Err(anyhow::anyhow!("import failed: {e}"));
        }
    };
    let import_id = db::imports::start(pool, source, source_url, file_name, &sha, &report.format).await?;
    match upsert_games(pool, import_id, &report).await {
        Ok(mut outcome) => {
            outcome.notes.extend(report.rejections.iter().cloned());
            let stats = ImportStats {
                rows_read: report.rows_read,
                games_inserted: outcome.games_inserted,
                games_updated: outcome.games_updated,
                games_unchanged: outcome.games_unchanged,
                rows_rejected: report.rows_rejected + outcome.rows_rejected - report.rows_rejected,
                min_season: outcome.seasons_touched.iter().min().copied(),
                max_season: outcome.seasons_touched.iter().max().copied(),
                notes: if outcome.notes.is_empty() { None } else { Some(outcome.notes.join("\n")) },
            };
            db::imports::finish(pool, import_id, &stats).await?;
            Ok(outcome)
        }
        Err(e) => {
            db::imports::fail(pool, import_id, &e.to_string()).await?;
            Err(e)
        }
    }
}

struct GameRow<'a> {
    home_id: i32,
    away_id: i32,
    game: &'a csv::ParsedGame,
}

/// Upsert one batch with a single `INSERT ... SELECT FROM UNNEST` statement.
/// Returns (inserted, updated, unchanged).
async fn upsert_batch(tx: &mut sqlx::Transaction<'_, sqlx::Postgres>, import_id: i64, rows: &[GameRow<'_>]) -> anyhow::Result<(i64, i64, i64)> {
    let n = rows.len();
    let season: Vec<i32> = rows.iter().map(|r| r.game.season).collect();
    let date: Vec<chrono::NaiveDate> = rows.iter().map(|r| r.game.date).collect();
    let seq: Vec<i16> = rows.iter().map(|r| r.game.seq).collect();
    let home: Vec<i32> = rows.iter().map(|r| r.home_id).collect();
    let away: Vec<i32> = rows.iter().map(|r| r.away_id).collect();
    let neutral: Vec<bool> = rows.iter().map(|r| r.game.neutral).collect();
    let gtype: Vec<GameType> = rows.iter().map(|r| if r.game.playoff { GameType::Playoff } else { GameType::Regular }).collect();
    let hs: Vec<Option<i32>> = rows.iter().map(|r| r.game.home_score).collect();
    let as_: Vec<Option<i32>> = rows.iter().map(|r| r.game.away_score).collect();
    let hpre: Vec<f64> = rows.iter().map(|r| r.game.home_elo_pre).collect();
    let apre: Vec<f64> = rows.iter().map(|r| r.game.away_elo_pre).collect();
    let hpost: Vec<f64> = rows.iter().map(|r| r.game.home_elo_post).collect();
    let apost: Vec<f64> = rows.iter().map(|r| r.game.away_elo_post).collect();
    let prob: Vec<Option<f64>> = rows.iter().map(|r| r.game.home_win_prob).collect();
    let returned: Vec<(bool,)> = sqlx::query_as(
        "INSERT INTO games (season, game_date, seq, home_team_id, away_team_id, neutral, game_type, home_score, away_score,
            home_elo_pre, away_elo_pre, home_elo_post, away_elo_post, home_win_prob, import_id)
         SELECT s, d, q, h, a, n, t, hs, as_, hpre, apre, hpost, apost, p, $15
         FROM UNNEST($1::int4[], $2::date[], $3::int2[], $4::int4[], $5::int4[], $6::bool[], $7::text[], $8::int4[], $9::int4[],
                     $10::float8[], $11::float8[], $12::float8[], $13::float8[], $14::float8[])
              AS u(s, d, q, h, a, n, t, hs, as_, hpre, apre, hpost, apost, p)
         ON CONFLICT (game_date, home_team_id, away_team_id, seq) DO UPDATE SET
            season = EXCLUDED.season, neutral = EXCLUDED.neutral,
            game_type = CASE WHEN games.game_type IN ('play_in', 'cup_final') AND EXCLUDED.game_type = 'regular' THEN games.game_type ELSE EXCLUDED.game_type END,
            home_score = EXCLUDED.home_score, away_score = EXCLUDED.away_score,
            home_elo_pre = EXCLUDED.home_elo_pre, away_elo_pre = EXCLUDED.away_elo_pre,
            home_elo_post = EXCLUDED.home_elo_post, away_elo_post = EXCLUDED.away_elo_post,
            home_win_prob = EXCLUDED.home_win_prob, import_id = EXCLUDED.import_id, updated_at = now()
         WHERE (games.season, games.neutral, games.home_score, games.away_score, games.home_elo_pre, games.away_elo_pre,
                games.home_elo_post, games.away_elo_post, games.home_win_prob,
                CASE WHEN games.game_type IN ('play_in', 'cup_final') THEN 'regular' ELSE games.game_type END)
               IS DISTINCT FROM
               (EXCLUDED.season, EXCLUDED.neutral, EXCLUDED.home_score, EXCLUDED.away_score, EXCLUDED.home_elo_pre,
                EXCLUDED.away_elo_pre, EXCLUDED.home_elo_post, EXCLUDED.away_elo_post, EXCLUDED.home_win_prob, EXCLUDED.game_type)
         RETURNING (xmax = 0) AS inserted",
    )
    .bind(&season)
    .bind(&date)
    .bind(&seq)
    .bind(&home)
    .bind(&away)
    .bind(&neutral)
    .bind(&gtype)
    .bind(&hs)
    .bind(&as_)
    .bind(&hpre)
    .bind(&apre)
    .bind(&hpost)
    .bind(&apost)
    .bind(&prob)
    .bind(import_id)
    .fetch_all(&mut **tx)
    .await?;
    let ins = returned.iter().filter(|r| r.0).count() as i64;
    let upd = returned.len() as i64 - ins;
    Ok((ins, upd, n as i64 - returned.len() as i64))
}

async fn upsert_games(pool: &Pool, import_id: i64, report: &csv::ParseReport) -> anyhow::Result<ImportOutcome> {
    let teams = db::teams::team_code_map(pool).await?;
    let mut unknown: HashSet<String> = HashSet::new();
    let mut rejected = 0i64;
    let mut seasons: HashSet<i32> = HashSet::new();
    // Ensure season rows exist first (FK)
    for g in &report.games {
        seasons.insert(g.season);
    }
    let mut season_list: Vec<i32> = seasons.iter().copied().collect();
    season_list.sort();
    for s in &season_list {
        sqlx::query("INSERT INTO seasons (season, label) VALUES ($1, $2) ON CONFLICT (season) DO NOTHING")
            .bind(s)
            .bind(reference::season_label(*s))
            .execute(pool)
            .await?;
    }
    let (mut inserted, mut updated, mut unchanged) = (0i64, 0i64, 0i64);
    let batch_size = 2000;
    let mut batch: Vec<GameRow<'_>> = Vec::with_capacity(batch_size);
    let mut tx = pool.begin().await?;
    for g in &report.games {
        match (teams.get(&g.home), teams.get(&g.away)) {
            (Some(h), Some(a)) => batch.push(GameRow { home_id: h.id, away_id: a.id, game: g }),
            _ => {
                rejected += 1;
                unknown.extend([&g.home, &g.away].into_iter().filter(|c| !teams.contains_key(*c)).cloned());
                continue;
            }
        }
        if batch.len() >= batch_size {
            let (i, u, s) = upsert_batch(&mut tx, import_id, &batch).await?;
            inserted += i;
            updated += u;
            unchanged += s;
            batch.clear();
        }
    }
    if !batch.is_empty() {
        let (i, u, s) = upsert_batch(&mut tx, import_id, &batch).await?;
        inserted += i;
        updated += u;
        unchanged += s;
    }
    tx.commit().await?;
    let mut notes = Vec::new();
    if !unknown.is_empty() {
        let mut u: Vec<String> = unknown.into_iter().collect();
        u.sort();
        warn!(?u, "unknown team codes skipped");
        notes.push(format!("unknown team codes skipped: {}", u.join(", ")));
    }
    info!(inserted, updated, unchanged, rejected, "games upserted");
    // Derived data
    db::teams::sync_alignments(pool).await?;
    derived::rebuild_seasons(pool, &season_list).await?;
    Ok(ImportOutcome {
        import_id,
        format: report.format.clone(),
        rows_read: report.rows_read,
        rows_rejected: report.rows_rejected + rejected,
        games_inserted: inserted,
        games_updated: updated,
        games_unchanged: unchanged,
        seasons_touched: season_list,
        skipped_duplicate: false,
        notes,
    })
}

/// Import a local file (optionally gzip-compressed).
pub async fn import_path(pool: &Pool, path: &Path, source: &str, force: bool) -> anyhow::Result<ImportOutcome> {
    let bytes = read_file(path)?;
    let name = path.file_name().map(|n| n.to_string_lossy().to_string());
    import_bytes(pool, &bytes, source, None, name.as_deref(), force).await
}

/// Download the configured source and import it. The downloaded file is kept in `data_dir`
/// for provenance.
pub async fn refresh_from_url(pool: &Pool, url: &str, data_dir: &Path, force: bool) -> anyhow::Result<ImportOutcome> {
    info!(url, "downloading dataset");
    let client = reqwest::Client::builder().timeout(std::time::Duration::from_secs(300)).build()?;
    let resp = client.get(url).send().await?.error_for_status()?;
    let bytes = resp.bytes().await?.to_vec();
    if bytes.len() < 100 {
        anyhow::bail!("downloaded file is too small to be a dataset ({} bytes)", bytes.len());
    }
    std::fs::create_dir_all(data_dir.join("downloads"))?;
    let stamp = chrono::Utc::now().format("%Y%m%dT%H%M%SZ");
    let file_name = format!("nba_elo_{stamp}.csv");
    let dest = data_dir.join("downloads").join(&file_name);
    std::fs::write(&dest, &bytes)?;
    import_bytes(pool, &bytes, "download", Some(url), Some(&file_name), force).await
}

pub fn franchise_map() -> HashMap<&'static str, &'static str> {
    reference::TEAM_LINEAGE.iter().map(|t| (t.0, t.1)).collect()
}
