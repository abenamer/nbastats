//! PostgreSQL integration tests against the real adapter: migrations, idempotent import,
//! derived data, bracket reconstruction, leakage guards, job lifecycle. Run with
//! `DATABASE_URL` set (REAS-TEST-001).

mod common;

use chrono::NaiveDate;
use nba_elo_simulator::application::imports::{self, Provenance};
use nba_elo_simulator::application::ports::{
    AnalysisStore, FetchProgress, FetchProgressSink, FetchWindow, GameLog, GameStore, ImportLog,
    JobOutcome, JobStore, NewJob, ResultRecord, ResultsFetch, ResultsSource, SourceError,
};
use nba_elo_simulator::domain::ids::{SeasonYear, TeamId};
use nba_elo_simulator::domain::models::{
    Conference, GameType, ImportStatus, JobKind, JobStatus, PlayoffResult, Team,
};
use nba_elo_simulator::domain::playoffs::reconstruct;
use nba_elo_simulator::domain::playoffs::season::{self, SeasonParams};
use nba_elo_simulator::domain::simulation::progress::Silent;
use sqlx::PgPool;

const S2016: SeasonYear = SeasonYear::of(2016);

fn prov<'a>(name: &'a str) -> Provenance<'a> {
    Provenance {
        source: "test",
        source_url: None,
        file_name: Some(name),
    }
}

#[sqlx::test(migrations = "./migrations")]
async fn migrations_apply_and_reference_data_syncs(pool: PgPool) {
    let (store, teams) = common::setup(pool).await;
    assert!(teams.contains_key("GSW"));
    assert!(teams.contains_key("SEA"));
    let h = store.health().await;
    assert!(h.ok);
    assert!(h.migrations_applied >= 1);
    // syncing twice is idempotent
    store.sync_reference().await.unwrap();
    assert_eq!(store.teams().await.unwrap().len(), teams.len());
}

#[sqlx::test(migrations = "./migrations")]
async fn import_is_idempotent_and_derives_history(pool: PgPool) {
    let (store, _) = common::setup(pool).await;
    let first = imports::import_bytes(
        &store,
        common::FIXTURE.as_bytes(),
        prov("mini_2016.csv"),
        false,
    )
    .await
    .unwrap();
    assert_eq!(first.rows_rejected, 0);
    assert!(
        first.games_inserted > 1300,
        "inserted {}",
        first.games_inserted
    );
    assert_eq!(first.games_updated, 0);

    // Identical file: skipped by checksum.
    let again = imports::import_bytes(
        &store,
        common::FIXTURE.as_bytes(),
        prov("mini_2016.csv"),
        false,
    )
    .await
    .unwrap();
    assert!(again.skipped_duplicate);

    // Forced re-import: every game unchanged, nothing duplicated.
    let forced = imports::import_bytes(
        &store,
        common::FIXTURE.as_bytes(),
        prov("mini_2016.csv"),
        true,
    )
    .await
    .unwrap();
    assert_eq!(forced.games_inserted, 0);
    assert_eq!(forced.games_updated, 0);
    assert_eq!(forced.games_unchanged, first.games_inserted);
    assert_eq!(store.game_count().await.unwrap(), first.games_inserted);

    // A changed score is an update, not a duplicate.
    let mut edited = common::FIXTURE.replace(
        "2016-06-19,2016,0,TRUE,GSW,CLE,1764.828667,1749.839437,0.659694076,0.340305924,1755.639,1759.029104,89,93,1",
        "2016-06-19,2016,0,TRUE,GSW,CLE,1764.828667,1749.839437,0.659694076,0.340305924,1755.639,1759.029104,90,93,1",
    );
    assert_ne!(edited, common::FIXTURE);
    edited.push('\n');
    let changed = imports::import_bytes(&store, edited.as_bytes(), prov("edited.csv"), false)
        .await
        .unwrap();
    assert_eq!(changed.games_updated, 1);
    assert_eq!(changed.games_inserted, 0);

    // Derived data
    let seasons = store.seasons().await.unwrap();
    assert_eq!(seasons.len(), 1);
    assert!(seasons[0].complete);
    assert_eq!(seasons[0].playoff_game_count, 86);
    let champs = store.championships(S2016, S2016).await.unwrap();
    assert_eq!(champs[0].franchise_key, "CLE");
    assert_eq!(champs[0].source, "derived");
    let ts = store.team_seasons(S2016).await.unwrap();
    assert_eq!(ts.len(), 30);
    let gsw = ts.iter().find(|t| t.code == "GSW").unwrap();
    assert_eq!((gsw.wins, gsw.losses), (73, 9));
    assert_eq!(gsw.seed, Some(1));
    assert_eq!(gsw.playoff_result, PlayoffResult::Finals);
    assert!(gsw.elo_pre_playoffs.unwrap() > 1750.0);
    assert_eq!(store.imports(10).await.unwrap().len(), 3);
}

#[sqlx::test(migrations = "./migrations")]
async fn malformed_csv_is_rejected_cleanly(pool: PgPool) {
    let (store, _) = common::setup(pool).await;
    let err = imports::import_bytes(
        &store,
        b"this,is,not,a,dataset\n1,2,3,4,5\n",
        prov("bad.csv"),
        false,
    )
    .await;
    assert!(err.is_err());
    let imports = store.imports(10).await.unwrap();
    assert_eq!(imports[0].status, ImportStatus::Failed);
    assert_eq!(store.game_count().await.unwrap(), 0);
    // Partially broken file: good rows imported, bad rows counted.
    let mut broken = common::FIXTURE.to_string();
    broken.push_str("not-a-date,2016,0,,GSW,CLE,1700,1700,0.5,0.5,1700,1700,1,2,1\n");
    broken.push_str("2016-01-01,2016,0,,ZZZ,CLE,1700,1700,0.5,0.5,1700,1700,1,2,1\n");
    let r = imports::import_bytes(&store, broken.as_bytes(), prov("broken.csv"), false)
        .await
        .unwrap();
    assert_eq!(r.rows_rejected, 2);
    assert!(r.notes.iter().any(|n| n.contains("ZZZ")));
}

#[sqlx::test(migrations = "./migrations")]
async fn bracket_reconstruction_matches_history(pool: PgPool) {
    let (store, teams) = common::setup(pool).await;
    imports::import_bytes(
        &store,
        common::FIXTURE.as_bytes(),
        prov("mini_2016.csv"),
        false,
    )
    .await
    .unwrap();
    let by_id = store.team_map().await.unwrap();
    let games = store.season_games(S2016).await.unwrap();
    let r = reconstruct::reconstruct(S2016, &games, &by_id).unwrap();
    assert!(r.complete);
    let code = |slot: usize| r.bracket.teams[slot].code.as_str();
    // East: CLE 1 v DET 8, ATL 4 v BOS 5, TOR 2 v IND 7, MIA 3 v CHO 6
    assert_eq!(
        [
            code(0),
            code(1),
            code(2),
            code(3),
            code(4),
            code(5),
            code(6),
            code(7)
        ],
        ["CLE", "DET", "ATL", "BOS", "TOR", "IND", "MIA", "CHO"]
    );
    // West: GSW 1 v HOU 8, LAC 4 v POR 5, SAS 2 v MEM 7, OKC 3 v DAL 6
    assert_eq!(
        [
            code(8),
            code(9),
            code(10),
            code(11),
            code(12),
            code(13),
            code(14),
            code(15)
        ],
        ["GSW", "HOU", "LAC", "POR", "SAS", "MEM", "OKC", "DAL"]
    );
    assert_eq!(
        r.bracket.teams[0].actual_result,
        Some(PlayoffResult::Champion)
    );
    assert_eq!(
        r.bracket.teams[8].actual_result,
        Some(PlayoffResult::Finals)
    );
    assert_eq!(r.bracket.actual_series.len(), 15);
    assert!(r.bracket.notes.is_empty(), "{:?}", r.bracket.notes);
    assert!(r.bracket.teams.iter().all(|t| teams.contains_key(&t.code)));
}

#[sqlx::test(migrations = "./migrations")]
async fn forecasts_never_use_future_information(pool: PgPool) {
    let (store, _) = common::setup(pool).await;
    imports::import_bytes(
        &store,
        common::FIXTURE.as_bytes(),
        prov("mini_2016.csv"),
        false,
    )
    .await
    .unwrap();
    let by_id = store.team_map().await.unwrap();
    let games = store.season_games(S2016).await.unwrap();
    let cutoff = NaiveDate::from_ymd_opt(2016, 1, 15).unwrap();
    // The query-level guard
    let until = store.season_games_until(S2016, cutoff).await.unwrap();
    assert!(until.iter().all(|g| g.game_date <= cutoff));
    assert!(until.len() < games.len());
    // The engine-level guard: identical forecast whether or not later games are present.
    let params = SeasonParams {
        season: S2016,
        as_of: Some(cutoff),
        iterations: 300,
        seed: 5,
        home_advantage: 100.0,
        regular_season_k: Some(20.0),
        playoff_k: None,
        playoff_scale: 1.0,
    };
    let full = season::run(&params, &games, &by_id, &Silent).unwrap();
    let truncated_games: Vec<_> = games
        .iter()
        .filter(|g| g.game_date <= cutoff || g.game_type == GameType::Regular)
        .cloned()
        .collect();
    let truncated = season::run(&params, &truncated_games, &by_id, &Silent).unwrap();
    for (a, b) in full.teams.iter().zip(&truncated.teams) {
        assert_eq!(a.code, b.code);
        assert_eq!(a.elo_as_of, b.elo_as_of);
        assert_eq!(a.p_champion, b.p_champion);
    }
    // A cutoff inside the playoffs is clamped back to the day before they started.
    let late = SeasonParams {
        as_of: Some(NaiveDate::from_ymd_opt(2016, 6, 1).unwrap()),
        ..params.clone()
    };
    let clamped = season::run(&late, &games, &by_id, &Silent).unwrap();
    assert_eq!(clamped.as_of, NaiveDate::from_ymd_opt(2016, 4, 15).unwrap());
    assert!(clamped.information_cutoff_note.contains("moved"));
    // Pre-playoff Elo for a forecast equals the rating after the last non-playoff game.
    let pre = reconstruct::pre_playoff_elo(&games);
    let gsw = by_id.values().find(|t| t.code == "GSW").unwrap();
    let last_regular = games
        .iter()
        .rfind(|g| !g.is_playoff() && g.involves(gsw.id))
        .unwrap();
    assert_eq!(pre[&gsw.id], last_regular.elo_of(gsw.id).1);
}

#[sqlx::test(migrations = "./migrations")]
async fn job_lifecycle_survives_restart(pool: PgPool) {
    let (store, _) = common::setup(pool).await;
    let params = serde_json::json!({"franchises": ["a", "b"], "alphas": [1, 1], "seasons": 3, "iterations": 10, "threshold": 1, "seed": 1});
    let job = store
        .create_job(&NewJob {
            kind: JobKind::Alpha,
            name: Some("scenario"),
            params: &params,
            seed: Some(1),
            app_version: "0.1.0",
            model_version: "alpha-lottery-1.0",
        })
        .await
        .unwrap();
    let claimed = store.claim_next_job().await.unwrap().unwrap();
    assert_eq!(claimed.id, job.id);
    assert_eq!(claimed.status, JobStatus::Running);
    assert!(store.claim_next_job().await.unwrap().is_none());
    // "Restart": the running job is marked interrupted but keeps its configuration.
    assert_eq!(store.mark_running_jobs_interrupted().await.unwrap(), 1);
    let j = store.job(job.id).await.unwrap().unwrap();
    assert_eq!(j.status, JobStatus::Interrupted);
    assert_eq!(j.params, params);
    // A stale outcome for a job that is no longer running is rejected.
    assert!(!store
        .finish_job(job.id, &JobOutcome::Completed(serde_json::json!({})))
        .await
        .unwrap());
    // Retry re-queues it; a completed job cannot be retried.
    assert!(store.retry_job(job.id).await.unwrap());
    let again = store.claim_next_job().await.unwrap().unwrap();
    assert_eq!(again.attempts, 2);
    assert!(store
        .finish_job(
            job.id,
            &JobOutcome::Completed(serde_json::json!({"ok": true}))
        )
        .await
        .unwrap());
    assert!(!store.retry_job(job.id).await.unwrap());
    let scenarios = store.scenarios(JobKind::Alpha).await.unwrap();
    assert_eq!(scenarios.len(), 1);
    assert_eq!(scenarios[0].name.as_deref(), Some("scenario"));
}

#[sqlx::test(migrations = "./migrations")]
async fn records_queries_summarise_the_game_log(pool: PgPool) {
    use nba_elo_simulator::domain::records;
    let (store, teams) = common::setup(pool).await;
    imports::import_bytes(
        &store,
        common::FIXTURE.as_bytes(),
        prov("mini_2016.csv"),
        false,
    )
    .await
    .unwrap();
    let stats = store.team_season_stats().await.unwrap();
    assert_eq!(stats.len(), 30);
    let gsw = stats.iter().find(|s| s.code == "GSW").unwrap();
    let games = store.season_games(S2016).await.unwrap();
    let gsw_id = teams["GSW"].id;
    let gsw_games: Vec<_> = games
        .iter()
        .filter(|g| g.involves(gsw_id) && g.is_played())
        .collect();
    assert_eq!(gsw.games as usize, gsw_games.len());
    let peak = gsw_games
        .iter()
        .map(|g| g.elo_of(gsw_id).1)
        .fold(f64::MIN, f64::max);
    assert!((gsw.peak - peak).abs() < 1e-9);
    assert!(gsw.low <= gsw.peak);
    assert_eq!(gsw.playoff_result, PlayoffResult::Finals);
    assert_eq!(gsw.conference, Some(Conference::West));
    // The largest swing is a real game: the winner gained what the loser lost.
    let swings = store.biggest_swings(5).await.unwrap();
    assert_eq!(swings.len(), 5);
    assert!(swings[0].elo_post - swings[0].elo_pre >= swings[1].elo_post - swings[1].elo_pre);
    assert!(swings
        .iter()
        .all(|s| s.points > s.opponent_points && s.elo_post > s.elo_pre));
    let rosters = store.season_rosters().await.unwrap();
    assert_eq!(rosters.len(), 30);
    assert!(rosters.iter().all(|(s, _)| *s == S2016));
    // Matchups come back ranked by the combined pre-game rating, both ways.
    let greatest = store
        .matchups(records::MatchupOrder::Strongest, 3)
        .await
        .unwrap();
    let weakest = store
        .matchups(records::MatchupOrder::Weakest, 3)
        .await
        .unwrap();
    assert_eq!((greatest.len(), weakest.len()), (3, 3));
    assert!(greatest[0].combined_elo() >= greatest[1].combined_elo());
    assert!(weakest[0].combined_elo() <= weakest[1].combined_elo());
    assert!(greatest[0].combined_elo() > weakest[0].combined_elo());
    // One weakest game per season and league: the fixture has one NBA season, no ABA games.
    let per_season = store.weakest_matchup_per_season().await.unwrap();
    assert_eq!(per_season.len(), 1);
    assert_eq!(per_season[0].season, S2016);
    assert!((per_season[0].combined_elo() - weakest[0].combined_elo()).abs() < 1e-9);
    // The champion's road comes from the championships table, with both phases filled.
    let roads = store.champion_roads().await.unwrap();
    assert_eq!(roads.len(), 1);
    assert_eq!((roads[0].code.as_str(), roads[0].season), ("CLE", S2016));
    assert!(roads[0].regular_games > 0 && roads[0].playoff_games > 0);
    assert!(roads[0].playoff_opponent_elo > roads[0].regular_opponent_elo);
    assert!(roads[0].toughest_opponent_elo >= roads[0].playoff_opponent_elo);
    let r = records::compute(
        &stats,
        records::GameRecords {
            swings,
            greatest_matchups: greatest,
            weakest_per_season: per_season,
            champion_roads: roads,
        },
        &rosters,
    );
    assert_eq!(r.greatest_matchups.len(), 3);
    assert_eq!(r.clankers_by_era.len(), 1);
    assert_eq!(r.clankers_by_era[0].game.season, S2016);
    assert_eq!(r.roads_by_era.len(), 1);
    assert_eq!(r.roads_by_era[0].hardest.code, "CLE");
    assert_eq!(r.all_time_high[0].code, "GSW");
    assert_eq!(r.best_champion[0].code, "CLE");
    assert_eq!(r.conference_seasons.len(), 1);
    assert!(r.conference_seasons[0].west_mean > 0.0 && r.conference_seasons[0].east_mean > 0.0);
    assert_eq!(r.divisions.best.len(), 6);
}

#[sqlx::test(migrations = "./migrations")]
async fn curated_championships_backfill_only_missing_seasons(pool: PgPool) {
    use nba_elo_simulator::application::imports::derived;
    let (store, _teams) = common::setup(pool).await;
    imports::import_bytes(
        &store,
        common::FIXTURE.as_bytes(),
        prov("mini_2016.csv"),
        false,
    )
    .await
    .unwrap();
    // 2016 is derived from complete playoff data; 1947 and 1956 have no games here yet.
    store.ensure_season(SeasonYear::of(1947)).await.unwrap();
    store.ensure_season(SeasonYear::of(1956)).await.unwrap();
    let inserted = derived::backfill_curated_championships(&store)
        .await
        .unwrap();
    assert_eq!(inserted, 2);
    let champs = store
        .championships(SeasonYear::MIN, SeasonYear::MAX)
        .await
        .unwrap();
    let c1947 = champs
        .iter()
        .find(|c| c.season == SeasonYear::of(1947))
        .unwrap();
    assert_eq!(
        (c1947.champion_code.as_str(), c1947.franchise_key.as_str()),
        ("PHW", "GSW")
    );
    assert_eq!(c1947.runner_up_code.as_deref(), Some("CHS"));
    assert_eq!(c1947.source, "curated");
    let c2016 = champs.iter().find(|c| c.season == S2016).unwrap();
    assert_eq!(
        (c2016.champion_code.as_str(), c2016.source.as_str()),
        ("CLE", "derived")
    );
    // Running it again changes nothing.
    assert_eq!(
        derived::backfill_curated_championships(&store)
            .await
            .unwrap(),
        0
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn computed_elo_continues_from_the_previous_season(pool: PgPool) {
    use nba_elo_simulator::application::continuation;
    use nba_elo_simulator::domain::elo;
    use nba_elo_simulator::domain::models::{EloSource, ResultSource};
    let (store, teams) = common::setup(pool).await;
    imports::import_bytes(
        &store,
        common::FIXTURE.as_bytes(),
        prov("mini_2016.csv"),
        false,
    )
    .await
    .unwrap();
    let s2017 = SeasonYear::of(2017);
    // Archive results stand: a results file whose only game is already in the archive adds
    // nothing, and a projection of a season with archive games is refused.
    let err = continuation::import_results(
        &store,
        S2016,
        b"date,home,away,home_score,away_score,neutral,playoff\n2016-06-19,GSW,CLE,89,93,0,1\n",
        None,
    )
    .await;
    assert!(matches!(
        err,
        Err(continuation::ContinuationUseCaseError::NoGames(s)) if s == S2016
    ));
    assert!(matches!(
        continuation::project_season(&store, S2016).await,
        Err(continuation::ContinuationUseCaseError::SeasonHasArchiveResults(_))
    ));
    // Completing an imported season: archive games are kept, later games chain from them.
    let imported_2016 = store.season_games(S2016).await.unwrap();
    let last_imported = imported_2016.iter().map(|g| g.game_date).max().unwrap();
    let last_gsw = imported_2016
        .iter()
        .rfind(|g| g.involves(teams["GSW"].id))
        .unwrap();
    let gsw_after_archive = last_gsw.elo_of(teams["GSW"].id).1;
    let after = last_imported + chrono::Duration::days(3);
    let completion = FakeResults::with(vec![
        // one game that is already in the archive, listed with home and away swapped
        // (sources disagree at neutral sites): skipped, not changed
        record(
            &last_gsw.game_date.to_string(),
            &store_code(&teams, last_gsw.away_team_id),
            &store_code(&teams, last_gsw.home_team_id),
            Some((1, 2)),
        ),
        record(&after.to_string(), "GSW", "CLE", Some((110, 100))),
    ]);
    let before = imported_2016.len();
    let today = after + chrono::Duration::days(2);
    let outcome = continuation::sync_results(&store, &completion, S2016, today, &|_| {})
        .await
        .unwrap();
    assert_eq!(outcome.games_inserted, 1);
    // The archive already has played games, so only the days after its last game are asked for.
    assert_eq!(
        completion.windows.lock().unwrap().as_slice(),
        [FetchWindow::between(last_imported, today)]
    );
    let games = store.season_games(S2016).await.unwrap();
    assert_eq!(games.len(), before + 1);
    assert_eq!(
        games
            .iter()
            .filter(|g| g.result_source == ResultSource::Archive)
            .count(),
        before
    );
    assert!(games.iter().all(|g| g.elo_source == EloSource::Computed));
    let added = games.last().unwrap();
    assert_eq!(added.elo_source, EloSource::Computed);
    assert!((added.home_elo_pre - gsw_after_archive).abs() < 1e-9);
    assert_eq!(
        store
            .season_games(S2016)
            .await
            .unwrap()
            .iter()
            .find(|g| g.id == last_gsw.id)
            .unwrap()
            .home_score,
        last_gsw.home_score
    );
    // Remove the computed tail again so the rest of the test sees the archive season as before.
    store
        .replace_season_games(S2016, outcome.import_id, &[], &[])
        .await
        .unwrap();
    assert_eq!(store.season_games(S2016).await.unwrap().len(), before);
    // Projection: synthetic schedule, reverted ratings, no results.
    let projected = continuation::project_season(&store, s2017).await.unwrap();
    assert_eq!(projected.games_inserted, 1230);
    let games = store.season_games(s2017).await.unwrap();
    assert!(games
        .iter()
        .all(|g| g.elo_source == EloSource::Computed && !g.is_played()));
    let gsw = teams["GSW"].id;
    let gsw_2016_end = store
        .season_games(S2016)
        .await
        .unwrap()
        .iter()
        .rfind(|g| g.involves(gsw))
        .unwrap()
        .elo_of(gsw)
        .1;
    let gsw_2017_start = games
        .iter()
        .find(|g| g.involves(gsw))
        .unwrap()
        .elo_of(gsw)
        .0;
    assert!((gsw_2017_start - elo::season_reversion(gsw_2016_end)).abs() < 1e-9);
    let summary = store
        .seasons()
        .await
        .unwrap()
        .into_iter()
        .find(|s| s.season == s2017)
        .unwrap();
    assert_eq!(summary.scheduled_game_count, 1230);
    assert_eq!(summary.computed_game_count, 1230);
    assert!(!summary.complete);
    // A season forecast runs on the projection.
    let by_id = store.team_map().await.unwrap();
    let params = SeasonParams {
        season: s2017,
        as_of: None,
        iterations: 200,
        seed: 1,
        home_advantage: 100.0,
        regular_season_k: Some(20.0),
        playoff_k: None,
        playoff_scale: 1.0,
    };
    let forecast = season::run(&params, &games, &by_id, &Silent).unwrap();
    assert_eq!(forecast.games_remaining, 1230);
    assert!((forecast.teams.iter().map(|t| t.p_champion).sum::<f64>() - 1.0).abs() < 1e-9);
    // Results replace the projection and chain ratings through played games.
    let csv = "date,home,away,home_score,away_score,neutral,playoff\n2016-10-25,GSW,SAS,100,129,0,0\n2016-10-28,GSW,NOP,122,114,0,0\n2016-10-30,DAL,GSW,,,0,0\n";
    let outcome = continuation::import_results(&store, s2017, csv.as_bytes(), Some("results.csv"))
        .await
        .unwrap();
    assert_eq!(outcome.games_inserted, 3);
    let games = store.season_games(s2017).await.unwrap();
    assert_eq!(games.len(), 3);
    let first = &games[0];
    assert_eq!((first.home_score, first.away_score), (Some(100), Some(129)));
    assert!(first.home_elo_post < first.home_elo_pre);
    assert!((games[1].home_elo_pre - first.home_elo_post).abs() < 1e-9);
    assert!(!games[2].is_played());
    assert_eq!(games[2].elo_of(gsw).0, games[1].elo_of(gsw).1);
    let ts = store.team_seasons(s2017).await.unwrap();
    let gsw_ts = ts.iter().find(|t| t.code == "GSW").unwrap();
    assert_eq!((gsw_ts.wins, gsw_ts.losses), (1, 1));

    // A results source (the BALLDONTLIE adapter in production) feeds the same path.
    let source = FakeResults::with(vec![
        record("2016-10-25", "GSW", "SAS", Some((100, 129))),
        record("2016-10-28", "GSW", "NOP", Some((122, 114))),
        record("2016-10-30", "DAL", "GSW", Some((95, 100))),
        record("2016-11-01", "GSW", "BRK", None),
    ]);
    let today = date("2016-11-02");
    let outcome = continuation::sync_results(&store, &source, s2017, today, &|_| {})
        .await
        .unwrap();
    assert_eq!(outcome.games_inserted, 4);
    assert!(outcome
        .notes
        .iter()
        .any(|n| n.contains("4 games fetched with 1 requests for 2016-10-28 to 2016-11-02")));
    // The store had played games through 2016-10-28, so the window starts there.
    assert_eq!(
        source.windows.lock().unwrap().as_slice(),
        [FetchWindow::between(date("2016-10-28"), today)]
    );
    let games = store.season_games(s2017).await.unwrap();
    assert_eq!(games.len(), 4);
    assert!(games.iter().all(|g| g.elo_source == EloSource::Computed));
    assert!(games[2].is_played() && !games[3].is_played());
    assert_eq!(games[3].elo_of(gsw).0, games[2].elo_of(gsw).1);
    let ts = store.team_seasons(s2017).await.unwrap();
    let gsw_ts = ts.iter().find(|t| t.code == "GSW").unwrap();
    assert_eq!((gsw_ts.wins, gsw_ts.losses), (2, 1));
    let last_import = store.imports(1).await.unwrap().remove(0);
    assert_eq!(last_import.source, "fake-results");
    assert_eq!(last_import.source_url.as_deref(), Some("fake://games"));
    // The next day's sync fetches only the window; earlier games stay as stored.
    let daily = FakeResults::with(vec![record("2016-11-01", "GSW", "BRK", Some((104, 95)))]);
    let today = date("2016-11-02");
    let outcome = continuation::sync_results(&store, &daily, s2017, today, &|_| {})
        .await
        .unwrap();
    assert_eq!(outcome.rows_read, 4);
    assert_eq!(
        daily.windows.lock().unwrap().as_slice(),
        [FetchWindow::between(date("2016-10-30"), today)]
    );
    let games = store.season_games(s2017).await.unwrap();
    assert_eq!(games.len(), 4);
    assert!(games.iter().all(|g| g.is_played()));
    assert!((games[3].home_elo_pre - games[2].elo_of(gsw).1).abs() < 1e-9);
    let ts = store.team_seasons(s2017).await.unwrap();
    let gsw_ts = ts.iter().find(|t| t.code == "GSW").unwrap();
    assert_eq!((gsw_ts.wins, gsw_ts.losses), (3, 1));
    // Unknown codes stop the sync before anything is written.
    let bad = FakeResults::with(vec![record("2016-10-25", "GSW", "XXX", None)]);
    let err = continuation::sync_results(&store, &bad, s2017, today, &|_| {}).await;
    assert!(matches!(
        err,
        Err(continuation::ContinuationUseCaseError::UnknownTeams(c)) if c == "XXX"
    ));
    assert_eq!(store.season_games(s2017).await.unwrap().len(), 4);
    // An empty fetch (a season the source does not have yet) stores nothing, not even a row.
    let s2018 = SeasonYear::of(2018);
    let empty = FakeResults::with(vec![]);
    assert!(matches!(
        continuation::sync_results(&store, &empty, s2018, today, &|_| {}).await,
        Err(continuation::ContinuationUseCaseError::NoGames(s)) if s == s2018
    ));
    // Nothing is stored for the season, so the whole season was asked for.
    assert_eq!(
        empty.windows.lock().unwrap().as_slice(),
        [FetchWindow::all()]
    );
    assert!(!store
        .seasons()
        .await
        .unwrap()
        .iter()
        .any(|s| s.season == s2018));
    // A computed season can be removed; an imported one cannot.
    assert_eq!(continuation::remove_season(&store, s2017).await.unwrap(), 4);
    assert!(!store
        .seasons()
        .await
        .unwrap()
        .iter()
        .any(|s| s.season == s2017));
    assert!(store.team_seasons(s2017).await.unwrap().is_empty());
    assert!(matches!(
        continuation::remove_season(&store, S2016).await,
        Err(continuation::ContinuationUseCaseError::SeasonHasArchiveResults(_))
    ));
}

struct FakeResults {
    records: Vec<ResultRecord>,
    /// The windows the use case asked for, in order.
    windows: std::sync::Mutex<Vec<FetchWindow>>,
}

impl FakeResults {
    fn with(records: Vec<ResultRecord>) -> Self {
        Self {
            records,
            windows: std::sync::Mutex::new(vec![]),
        }
    }
}

impl ResultsSource for FakeResults {
    fn name(&self) -> &'static str {
        "fake-results"
    }
    async fn season_results(
        &self,
        _season: SeasonYear,
        window: FetchWindow,
        progress: &dyn FetchProgressSink,
    ) -> Result<ResultsFetch, SourceError> {
        self.windows.lock().unwrap().push(window);
        progress.report(FetchProgress {
            requests: 1,
            games: self.records.len(),
            ..Default::default()
        });
        Ok(ResultsFetch {
            records: self.records.clone(),
            source_url: "fake://games".to_string(),
            requests: 1,
        })
    }
}

fn store_code(teams: &std::collections::HashMap<String, Team>, id: TeamId) -> String {
    teams
        .values()
        .find(|t| t.id == id)
        .map(|t| t.code.clone())
        .unwrap()
}

fn date(s: &str) -> NaiveDate {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
}

fn record(date: &str, home: &str, away: &str, score: Option<(i32, i32)>) -> ResultRecord {
    ResultRecord {
        date: NaiveDate::parse_from_str(date, "%Y-%m-%d").unwrap(),
        home_code: home.to_string(),
        away_code: away.to_string(),
        neutral: false,
        playoff: false,
        home_score: score.map(|s| s.0),
        away_score: score.map(|s| s.1),
    }
}

/// The application's Elo code re-rates an archive season to the archive's own numbers: the
/// one-step update from each game's imported pre-game ratings reproduces its post-game
/// ratings, and the chain from the first ratings reproduces the season-end ratings.
#[sqlx::test(migrations = "./migrations")]
async fn elo_replay_reproduces_the_archive_ratings(pool: PgPool) {
    use nba_elo_simulator::domain::continuation::{rate_games, ResultGame};
    use nba_elo_simulator::domain::elo;
    let (store, _teams) = common::setup(pool).await;
    imports::import_bytes(
        &store,
        common::FIXTURE.as_bytes(),
        prov("mini_2016.csv"),
        false,
    )
    .await
    .unwrap();
    let games = store.season_games(S2016).await.unwrap();
    assert!(
        games.len() > 100,
        "fixture season has {} games",
        games.len()
    );
    let mut one_step_max: f64 = 0.0;
    for g in games.iter().filter(|g| g.is_played()) {
        let (h, a) = (g.home_score.unwrap(), g.away_score.unwrap());
        let (hp, ap) = elo::fivethirtyeight_update(g.home_elo_pre, g.away_elo_pre, h, a, g.neutral);
        one_step_max = one_step_max
            .max((hp - g.home_elo_post).abs())
            .max((ap - g.away_elo_post).abs());
    }
    assert!(one_step_max < 0.1, "one-step max difference {one_step_max}");
    let mut start = std::collections::HashMap::new();
    for g in &games {
        start.entry(g.home_team_id).or_insert(g.home_elo_pre);
        start.entry(g.away_team_id).or_insert(g.away_elo_pre);
    }
    let input: Vec<ResultGame> = games
        .iter()
        .map(|g| ResultGame {
            date: g.game_date,
            home: g.home_team_id,
            away: g.away_team_id,
            neutral: g.neutral,
            playoff: g.game_type == GameType::Playoff,
            home_score: g.home_score,
            away_score: g.away_score,
        })
        .collect();
    let rated = rate_games(&start, &input).unwrap();
    let mut ours = std::collections::HashMap::new();
    for r in &rated {
        ours.insert(r.game.home, r.home_elo_post);
        ours.insert(r.game.away, r.away_elo_post);
    }
    let mut theirs = std::collections::HashMap::new();
    for g in &games {
        theirs.insert(g.home_team_id, g.home_elo_post);
        theirs.insert(g.away_team_id, g.away_elo_post);
    }
    let chain_max = theirs
        .iter()
        .map(|(t, e)| (ours[t] - e).abs())
        .fold(0.0_f64, f64::max);
    assert!(
        chain_max < 0.1,
        "season-end chain max difference {chain_max}"
    );
}
