//! PostgreSQL integration tests: migrations, idempotent import, derived data, bracket
//! reconstruction, leakage guards, job lifecycle. Run with `DATABASE_URL` set.

mod common;

use chrono::NaiveDate;
use nba_elo_simulator::adapters::postgres as db;
use nba_elo_simulator::application::imports;
use nba_elo_simulator::domain::models::{GameType, JobKind, PlayoffResult};
use nba_elo_simulator::domain::playoffs::reconstruct;
use nba_elo_simulator::domain::playoffs::season::{self, SeasonParams};
use nba_elo_simulator::domain::simulation::progress::Silent;
use sqlx::PgPool;

#[sqlx::test(migrations = "./migrations")]
async fn migrations_apply_and_reference_data_syncs(pool: PgPool) {
    let teams = common::setup(&pool).await;
    assert!(teams.contains_key("GSW"));
    assert!(teams.contains_key("SEA"));
    let h = db::health(&pool).await;
    assert!(h.ok);
    assert!(h.migrations_applied >= 1);
    // syncing twice is idempotent
    db::teams::sync_reference(&pool).await.unwrap();
    assert_eq!(db::teams::all_teams(&pool).await.unwrap().len(), teams.len());
}

#[sqlx::test(migrations = "./migrations")]
async fn import_is_idempotent_and_derives_history(pool: PgPool) {
    common::setup(&pool).await;
    let first = imports::import_bytes(&pool, common::FIXTURE.as_bytes(), "test", None, Some("mini_2016.csv"), false).await.unwrap();
    assert_eq!(first.rows_rejected, 0);
    assert!(first.games_inserted > 1300, "inserted {}", first.games_inserted);
    assert_eq!(first.games_updated, 0);

    // Identical file: skipped by checksum.
    let again = imports::import_bytes(&pool, common::FIXTURE.as_bytes(), "test", None, Some("mini_2016.csv"), false).await.unwrap();
    assert!(again.skipped_duplicate);

    // Forced re-import: every game unchanged, nothing duplicated.
    let forced = imports::import_bytes(&pool, common::FIXTURE.as_bytes(), "test", None, Some("mini_2016.csv"), true).await.unwrap();
    assert_eq!(forced.games_inserted, 0);
    assert_eq!(forced.games_updated, 0);
    assert_eq!(forced.games_unchanged, first.games_inserted);
    assert_eq!(db::games::game_count(&pool).await.unwrap(), first.games_inserted);

    // A changed score is an update, not a duplicate.
    let mut edited = common::FIXTURE.replace("2016-06-19,2016,0,TRUE,GSW,CLE,1764.828667,1749.839437,0.659694076,0.340305924,1755.639,1759.029104,89,93,1",
        "2016-06-19,2016,0,TRUE,GSW,CLE,1764.828667,1749.839437,0.659694076,0.340305924,1755.639,1759.029104,90,93,1");
    assert_ne!(edited, common::FIXTURE);
    edited.push('\n');
    let changed = imports::import_bytes(&pool, edited.as_bytes(), "test", None, Some("edited.csv"), false).await.unwrap();
    assert_eq!(changed.games_updated, 1);
    assert_eq!(changed.games_inserted, 0);

    // Derived data
    let seasons = db::games::seasons(&pool).await.unwrap();
    assert_eq!(seasons.len(), 1);
    assert!(seasons[0].complete);
    assert_eq!(seasons[0].playoff_game_count, 86);
    let champs = db::analysis::championships(&pool, 2016, 2016).await.unwrap();
    assert_eq!(champs[0].franchise_key, "CLE");
    assert_eq!(champs[0].source, "derived");
    let ts = db::analysis::team_seasons(&pool, 2016).await.unwrap();
    assert_eq!(ts.len(), 30);
    let gsw = ts.iter().find(|t| t.code == "GSW").unwrap();
    assert_eq!((gsw.wins, gsw.losses), (73, 9));
    assert_eq!(gsw.seed, Some(1));
    assert_eq!(gsw.playoff_result, PlayoffResult::Finals);
    assert!(gsw.elo_pre_playoffs.unwrap() > 1750.0);
    let imports = db::imports::list(&pool, 10).await.unwrap();
    assert_eq!(imports.len(), 3);
}

#[sqlx::test(migrations = "./migrations")]
async fn malformed_csv_is_rejected_cleanly(pool: PgPool) {
    common::setup(&pool).await;
    let err = imports::import_bytes(&pool, b"this,is,not,a,dataset\n1,2,3,4,5\n", "test", None, Some("bad.csv"), false).await;
    assert!(err.is_err());
    let imports = db::imports::list(&pool, 10).await.unwrap();
    assert_eq!(imports[0].status, nba_elo_simulator::domain::models::ImportStatus::Failed);
    assert_eq!(db::games::game_count(&pool).await.unwrap(), 0);
    // Partially broken file: good rows imported, bad rows counted.
    let mut broken = common::FIXTURE.to_string();
    broken.push_str("not-a-date,2016,0,,GSW,CLE,1700,1700,0.5,0.5,1700,1700,1,2,1\n");
    broken.push_str("2016-01-01,2016,0,,ZZZ,CLE,1700,1700,0.5,0.5,1700,1700,1,2,1\n");
    let r = imports::import_bytes(&pool, broken.as_bytes(), "test", None, Some("broken.csv"), false).await.unwrap();
    assert_eq!(r.rows_rejected, 2);
    assert!(r.notes.iter().any(|n| n.contains("ZZZ")));
}

#[sqlx::test(migrations = "./migrations")]
async fn bracket_reconstruction_matches_history(pool: PgPool) {
    let teams = common::setup(&pool).await;
    imports::import_bytes(&pool, common::FIXTURE.as_bytes(), "test", None, None, false).await.unwrap();
    let by_id = db::teams::team_map(&pool).await.unwrap();
    let games = db::games::season_games(&pool, 2016).await.unwrap();
    let r = reconstruct::reconstruct(2016, &games, &by_id).unwrap();
    assert!(r.complete);
    let code = |slot: usize| r.bracket.teams[slot].code.as_str();
    // East: CLE 1 v DET 8, ATL 4 v BOS 5, TOR 2 v IND 7, MIA 3 v CHO 6
    assert_eq!([code(0), code(1), code(2), code(3), code(4), code(5), code(6), code(7)], ["CLE", "DET", "ATL", "BOS", "TOR", "IND", "MIA", "CHO"]);
    // West: GSW 1 v HOU 8, LAC 4 v POR 5, SAS 2 v MEM 7, OKC 3 v DAL 6
    assert_eq!([code(8), code(9), code(10), code(11), code(12), code(13), code(14), code(15)], ["GSW", "HOU", "LAC", "POR", "SAS", "MEM", "OKC", "DAL"]);
    assert_eq!(r.bracket.teams[0].actual_result, Some(PlayoffResult::Champion));
    assert_eq!(r.bracket.teams[8].actual_result, Some(PlayoffResult::Finals));
    assert_eq!(r.bracket.actual_series.len(), 15);
    // 16 teams, each a known team
    assert!(r.bracket.teams.iter().all(|t| teams.contains_key(&t.code)));
}

#[sqlx::test(migrations = "./migrations")]
async fn forecasts_never_use_future_information(pool: PgPool) {
    common::setup(&pool).await;
    imports::import_bytes(&pool, common::FIXTURE.as_bytes(), "test", None, None, false).await.unwrap();
    let by_id = db::teams::team_map(&pool).await.unwrap();
    let games = db::games::season_games(&pool, 2016).await.unwrap();
    let cutoff = NaiveDate::from_ymd_opt(2016, 1, 15).unwrap();
    // The query-level guard
    let until = db::games::season_games_until(&pool, 2016, cutoff).await.unwrap();
    assert!(until.iter().all(|g| g.game_date <= cutoff));
    assert!(until.len() < games.len());
    // The engine-level guard: identical forecast whether or not later games are present.
    let params = SeasonParams { season: 2016, as_of: Some(cutoff), iterations: 300, seed: 5, home_advantage: 100.0, regular_season_k: Some(20.0), playoff_k: None, playoff_scale: 1.0 };
    let full = season::run(&params, &games, &by_id, &Silent).unwrap();
    let truncated_games: Vec<_> = games.iter().filter(|g| g.game_date <= cutoff || g.game_type == GameType::Regular).cloned().collect();
    let truncated = season::run(&params, &truncated_games, &by_id, &Silent).unwrap();
    for (a, b) in full.teams.iter().zip(&truncated.teams) {
        assert_eq!(a.code, b.code);
        assert_eq!(a.elo_as_of, b.elo_as_of);
        assert_eq!(a.p_champion, b.p_champion);
    }
    // A cutoff inside the playoffs is clamped back to the day before they started.
    let late = SeasonParams { as_of: Some(NaiveDate::from_ymd_opt(2016, 6, 1).unwrap()), ..params.clone() };
    let clamped = season::run(&late, &games, &by_id, &Silent).unwrap();
    assert_eq!(clamped.as_of, NaiveDate::from_ymd_opt(2016, 4, 15).unwrap());
    assert!(clamped.information_cutoff_note.contains("moved"));
    // Pre-playoff Elo for a forecast equals the rating after the last non-playoff game.
    let pre = reconstruct::pre_playoff_elo(&games);
    let gsw = by_id.values().find(|t| t.code == "GSW").unwrap();
    let last_regular = games.iter().rfind(|g| !g.is_playoff() && g.involves(gsw.id)).unwrap();
    assert_eq!(pre[&gsw.id], last_regular.elo_of(gsw.id).1);
}

#[sqlx::test(migrations = "./migrations")]
async fn job_lifecycle_survives_restart(pool: PgPool) {
    let params = serde_json::json!({"franchises": ["a", "b"], "alphas": [1, 1], "seasons": 3, "iterations": 10, "threshold": 1, "seed": 1});
    let job = db::jobs::create(&pool, JobKind::Alpha, Some("scenario"), &params, Some(1), "0.1.0", "alpha-lottery-1.0").await.unwrap();
    let claimed = db::jobs::claim_next(&pool).await.unwrap().unwrap();
    assert_eq!(claimed.id, job.id);
    assert_eq!(claimed.status, nba_elo_simulator::domain::models::JobStatus::Running);
    assert!(db::jobs::claim_next(&pool).await.unwrap().is_none());
    // "Restart": the running job is marked interrupted but keeps its configuration.
    assert_eq!(db::jobs::mark_interrupted(&pool).await.unwrap(), 1);
    let j = db::jobs::get(&pool, job.id).await.unwrap().unwrap();
    assert_eq!(j.status, nba_elo_simulator::domain::models::JobStatus::Interrupted);
    assert_eq!(j.params, params);
    // Retry re-queues it; a completed job cannot be retried.
    assert!(db::jobs::retry(&pool, job.id).await.unwrap());
    let again = db::jobs::claim_next(&pool).await.unwrap().unwrap();
    assert_eq!(again.attempts, 2);
    db::jobs::complete(&pool, job.id, &serde_json::json!({"ok": true})).await.unwrap();
    assert!(!db::jobs::retry(&pool, job.id).await.unwrap());
    let scenarios = db::jobs::scenarios(&pool, JobKind::Alpha).await.unwrap();
    assert_eq!(scenarios.len(), 1);
    assert_eq!(scenarios[0].name.as_deref(), Some("scenario"));
}
