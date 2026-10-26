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
        regular_home_advantage: None,
        regular_season_k: Some(20.0),
        playoff_k: None,
        playoff_scale: 1.0,
        absences: vec![],
    };
    let full = season::run(&params, &games, &by_id, &Silent).unwrap();
    // The projected bracket: sixteen distinct teams, seeds 1 to 8 per conference, exact
    // title chances that sum to one, and a seeding at most as common as its field.
    let pb = full.projected_bracket.as_ref().expect("a bracket");
    let mut ids: Vec<_> = pb.bracket.teams.iter().map(|t| t.team_id).collect();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), 16);
    for conf in 0..2 {
        let mut seeds: Vec<u8> = pb.bracket.teams[conf * 8..conf * 8 + 8]
            .iter()
            .map(|t| t.seed)
            .collect();
        seeds.sort_unstable();
        assert_eq!(seeds, (1..=8).collect::<Vec<u8>>());
    }
    assert!((pb.exact.iter().map(|e| e[3]).sum::<f64>() - 1.0).abs() < 1e-9);
    // Slots follow the bracket order; each first-round pair shares a conference.
    use nba_elo_simulator::domain::playoffs::bracket::SLOT_SEEDS;
    for (s, t) in pb.bracket.teams.iter().enumerate() {
        assert_eq!(t.seed, SLOT_SEEDS[s % 8]);
    }
    assert_eq!(pb.first_round.len(), 8);
    for (k, fr) in pb.first_round.iter().enumerate() {
        assert_eq!((fr.slot_a, fr.slot_b), (2 * k, 2 * k + 1));
        assert!((0.0..=1.0).contains(&fr.p_a));
        assert_eq!(
            pb.bracket.teams[fr.slot_a].conference,
            pb.bracket.teams[fr.slot_b].conference
        );
    }
    // Every completed season was sampled (300 iterations are within the sample), the most
    // frequent seeding occurred at least once, and the projected field occurred at least
    // as often as the projected seeding.
    assert_eq!(pb.sampled, full.iterations);
    assert!(pb.most_common_count >= 1 && pb.most_common_count >= pb.exact_count);
    assert!(pb.field_count >= pb.exact_count);
    assert!((pb.p_most_common - pb.most_common_count as f64 / pb.sampled as f64).abs() < 1e-12);
    // Each seed is held by one team per conference per completed season, and a team's
    // seed chances add up to its playoff chance.
    for conf in [Conference::East, Conference::West] {
        for k in 0..8 {
            let held: f64 = full
                .teams
                .iter()
                .filter(|t| t.conference == conf)
                .map(|t| t.seed_distribution[k])
                .sum();
            assert!(
                (held - 1.0).abs() < 1e-9,
                "seed {} of {conf:?}: {held}",
                k + 1
            );
        }
    }
    assert!(full
        .teams
        .iter()
        .all(|t| (t.seed_distribution.iter().sum::<f64>() - t.p_playoffs).abs() < 1e-9));
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
    // The 2015-16 Warriors went 15-9 in the playoffs; the champion Cavaliers 16-5.
    assert_eq!((gsw.playoff_wins, gsw.playoff_losses), (15, 9));
    let peak_game = gsw_games
        .iter()
        .max_by(|a, b| {
            a.elo_of(gsw_id)
                .1
                .total_cmp(&b.elo_of(gsw_id).1)
                .then(b.game_date.cmp(&a.game_date))
        })
        .unwrap();
    assert_eq!(gsw.peak_game_type, peak_game.game_type);
    let low_game = gsw_games
        .iter()
        .min_by(|a, b| {
            a.elo_of(gsw_id)
                .1
                .total_cmp(&b.elo_of(gsw_id).1)
                .then(a.game_date.cmp(&b.game_date))
        })
        .unwrap();
    assert_eq!(gsw.low_game_type, low_game.game_type);
    let cle = stats.iter().find(|s| s.code == "CLE").unwrap();
    assert_eq!(
        cle.peak_game_type,
        GameType::Playoff,
        "the champion peaked in the playoffs"
    );
    assert_eq!(cle.low_game_type, GameType::Regular);
    assert_eq!((cle.playoff_wins, cle.playoff_losses), (16, 5));
    let peak = gsw_games
        .iter()
        .map(|g| g.elo_of(gsw_id).1)
        .fold(f64::MIN, f64::max);
    assert!((gsw.peak - peak).abs() < 1e-9);
    assert!(gsw.low <= gsw.peak);
    assert_eq!(gsw.playoff_result, PlayoffResult::Finals);
    assert_eq!(gsw.conference, Some(Conference::West));
    // The largest swing is a real game: the winner gained what the loser lost.
    let swings = store.biggest_swings(None, 5).await.unwrap();
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
        .matchups(None, records::MatchupOrder::Strongest, 3)
        .await
        .unwrap();
    let weakest = store
        .matchups(None, records::MatchupOrder::Weakest, 3)
        .await
        .unwrap();
    assert_eq!((greatest.len(), weakest.len()), (3, 3));
    assert!(greatest[0].combined_elo() >= greatest[1].combined_elo());
    assert!(weakest[0].combined_elo() <= weakest[1].combined_elo());
    assert!(greatest[0].combined_elo() > weakest[0].combined_elo());
    // One weakest game per season and league: the fixture has one NBA season, no ABA games.
    let per_season = store.weakest_matchup_per_season(None).await.unwrap();
    assert_eq!(per_season.len(), 1);
    assert_eq!(per_season[0].season, S2016);
    assert!((per_season[0].combined_elo() - weakest[0].combined_elo()).abs() < 1e-9);
    // The champion's road comes from the championships table, with both phases filled.
    let roads = store.champion_roads(None).await.unwrap();
    assert_eq!(roads.len(), 1);
    assert_eq!((roads[0].code.as_str(), roads[0].season), ("CLE", S2016));
    assert!(roads[0].regular_games > 0 && roads[0].playoff_games > 0);
    assert!(roads[0].playoff_opponent_elo > roads[0].regular_opponent_elo);
    assert!(roads[0].toughest_opponent_elo >= roads[0].playoff_opponent_elo);
    // A season scope keeps the games of its seasons and leagues only: the fixture's one
    // NBA season answers in full under a scope that names it and not at all under one that
    // names another season or puts 2016 on the ABA side.
    let in_scope = records::SeasonScope {
        nba: vec![S2016],
        aba: vec![],
    };
    let elsewhere = records::SeasonScope {
        nba: vec![SeasonYear::of(2015)],
        aba: vec![S2016],
    };
    assert_eq!(
        store.biggest_swings(Some(&in_scope), 5).await.unwrap(),
        swings
    );
    assert!(store
        .biggest_swings(Some(&elsewhere), 5)
        .await
        .unwrap()
        .is_empty());
    assert_eq!(
        store
            .matchups(Some(&in_scope), records::MatchupOrder::Strongest, 3)
            .await
            .unwrap(),
        greatest
    );
    assert!(store
        .matchups(Some(&elsewhere), records::MatchupOrder::Strongest, 3)
        .await
        .unwrap()
        .is_empty());
    assert_eq!(
        store
            .weakest_matchup_per_season(Some(&in_scope))
            .await
            .unwrap(),
        per_season
    );
    assert!(store
        .weakest_matchup_per_season(Some(&elsewhere))
        .await
        .unwrap()
        .is_empty());
    assert_eq!(store.champion_roads(Some(&in_scope)).await.unwrap(), roads);
    assert!(store
        .champion_roads(Some(&elsewhere))
        .await
        .unwrap()
        .is_empty());
    // Two ABA games of 1975-76 take the ABA side of the scope: an ABA scope returns them
    // and nothing else, the NBA scope of 2016 still leaves them out. Kentucky and St. Louis
    // folded at the merger, so no NBA team continues from them and the 2016 ratings stay.
    let aba_csv = "date,season,neutral,playoff,team1,team2,elo1_pre,elo2_pre,elo_prob1,elo_prob2,elo1_post,elo2_post,score1,score2,is_home\n\
1975-11-01,1976,0,,KEN,SSL,1500,1500,0.6,0.4,1510,1490,110,100,1\n\
1975-11-03,1976,0,,SSL,KEN,1490,1510,0.5,0.5,1505,1495,120,105,1\n";
    imports::import_bytes(&store, aba_csv.as_bytes(), prov("mini_1976.csv"), false)
        .await
        .unwrap();
    let s1976 = SeasonYear::of(1976);
    let aba_scope = records::SeasonScope {
        nba: vec![],
        aba: vec![s1976],
    };
    let aba_swings = store.biggest_swings(Some(&aba_scope), 5).await.unwrap();
    assert_eq!(aba_swings.len(), 2);
    assert!(aba_swings.iter().all(|g| g.season == s1976));
    let aba_games = store
        .matchups(Some(&aba_scope), records::MatchupOrder::Strongest, 5)
        .await
        .unwrap();
    assert_eq!(aba_games.len(), 2);
    assert!(aba_games
        .iter()
        .all(|m| m.home_code == "KEN" || m.home_code == "SSL"));
    let aba_weakest = store
        .weakest_matchup_per_season(Some(&aba_scope))
        .await
        .unwrap();
    assert_eq!(aba_weakest.len(), 1);
    assert_eq!(aba_weakest[0].season, s1976);
    // The same season on the NBA side selects nothing: a game belongs to its teams' league.
    let wrong_side = records::SeasonScope {
        nba: vec![s1976],
        aba: vec![],
    };
    assert!(store
        .biggest_swings(Some(&wrong_side), 5)
        .await
        .unwrap()
        .is_empty());
    assert_eq!(
        store
            .weakest_matchup_per_season(Some(&in_scope))
            .await
            .unwrap(),
        per_season
    );
    assert_eq!(
        store.weakest_matchup_per_season(None).await.unwrap().len(),
        2
    );
    // The record tables over no seasons at all are empty, not a panic.
    let empty = records::compute(&[], records::GameRecords::default(), &[]);
    assert!(empty.all_time_high.is_empty() && empty.by_era.is_empty() && empty.by_cba.is_empty());
    assert!(empty.conference_seasons.is_empty() && empty.roads_by_era.is_empty());
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
        regular_home_advantage: None,
        regular_season_k: Some(20.0),
        playoff_k: None,
        playoff_scale: 1.0,
        absences: vec![],
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
    // The three stored games are updated in place (the DAL game gets its result); only
    // the new game is inserted.
    assert_eq!(outcome.games_inserted, 1);
    assert!(outcome.games_updated >= 1);
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
        cup_final: false,
        home_score: score.map(|s| s.0),
        away_score: score.map(|s| s.1),
    }
}

/// A synced game the source marks as the NBA Cup final keeps that type through the
/// derived rebuild (whose date rule needs complete totals) and through the next sync,
/// whose window no longer holds it. A stored play-in game round-trips the same way.
#[sqlx::test(migrations = "./migrations")]
async fn cup_final_from_the_source_survives_the_rebuild_and_the_next_sync(pool: PgPool) {
    use nba_elo_simulator::application::continuation;
    let (store, _teams) = common::setup(pool).await;
    imports::import_bytes(
        &store,
        common::FIXTURE.as_bytes(),
        prov("mini_2016.csv"),
        false,
    )
    .await
    .unwrap();
    let s2017 = SeasonYear::of(2017);
    let mut final_game = record("2016-12-17", "GSW", "SAS", Some((100, 90)));
    final_game.cup_final = true;
    let first = FakeResults::with(vec![
        record("2016-10-25", "GSW", "SAS", Some((100, 129))),
        final_game,
        record("2016-12-20", "DAL", "GSW", None),
    ]);
    continuation::sync_results(&store, &first, s2017, date("2016-12-18"), &|_| {})
        .await
        .unwrap();
    let games = store.season_games(s2017).await.unwrap();
    assert_eq!(
        games[1].game_type,
        GameType::CupFinal,
        "stored as the Cup final"
    );
    assert_eq!(games[0].game_type, GameType::Regular);
    // The next sync's fetch holds only the new result; the final is a stored game kept
    // through the merge, and it keeps its type (the old mapping made it a playoff game).
    let next = FakeResults::with(vec![record("2016-12-20", "DAL", "GSW", Some((95, 100)))]);
    continuation::sync_results(&store, &next, s2017, date("2016-12-21"), &|_| {})
        .await
        .unwrap();
    let games = store.season_games(s2017).await.unwrap();
    assert_eq!(games.len(), 3);
    assert_eq!(games[0].game_type, GameType::Regular);
    assert_eq!(games[1].game_type, GameType::CupFinal);
    assert!(games[2].is_played());
    let summary = store
        .seasons()
        .await
        .unwrap()
        .into_iter()
        .find(|s| s.season == s2017)
        .unwrap();
    assert_eq!(summary.cup_final_game_count, 1);
    assert_eq!(summary.playoff_game_count, 0);
    let ts = store.team_seasons(s2017).await.unwrap();
    let gsw = ts.iter().find(|t| t.code == "GSW").unwrap();
    assert_eq!(
        (gsw.wins, gsw.losses),
        (1, 1),
        "the Cup final is not in the record"
    );
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
            game_type: g.game_type,
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

/// The question box's tools read the store: every tool answers from the fixture season and
/// unknown names or codes come back as errors the model can correct, not as failures.
#[sqlx::test(migrations = "./migrations")]
async fn question_box_tools_read_the_store(pool: PgPool) {
    use nba_elo_simulator::application::ask::{self, Tools};
    use nba_elo_simulator::application::ports::{
        ChatReply, ChatRequest, LanguageModel, ModelError, ToolCall,
    };
    use serde_json::{json, Value};
    use std::sync::Mutex;

    let (store, _teams) = common::setup(pool).await;
    imports::import_bytes(
        &store,
        common::FIXTURE.as_bytes(),
        prov("mini_2016.csv"),
        false,
    )
    .await
    .unwrap();

    let tools = Tools::new(&store, 100.0);
    let call = |name: &str, args: Value| ToolCall {
        id: name.to_string(),
        name: name.to_string(),
        arguments: args,
    };
    let seasons = tools.call(call("list_seasons", json!({}))).await.unwrap();
    assert_eq!(seasons["count"], 1);
    assert_eq!(seasons["seasons"][0]["season"], 2016);
    let standings = tools
        .call(call("season_standings", json!({ "season": "2015-16" })))
        .await
        .unwrap();
    let teams = standings["teams"].as_array().unwrap();
    assert_eq!(teams.len(), 30);
    assert_eq!(teams[0]["code"], "GSW", "best record first");
    assert_eq!(teams[0]["wins"], 73);
    let champions = tools
        .call(call("champions", json!({ "from": 2016, "to": 2016 })))
        .await
        .unwrap();
    assert_eq!(champions["champions"][0]["champion"], "CLE");
    let history = tools
        .call(call("franchise_history", json!({ "franchise": "gsw" })))
        .await
        .unwrap();
    assert_eq!(history["count"], 1);
    assert_eq!(history["seasons"][0]["playoff_result"], "finals");
    let games = tools
        .call(call(
            "team_games",
            json!({ "season": 2016, "team": "CLE", "playoffs_only": true }),
        ))
        .await
        .unwrap();
    assert_eq!(games["wins"], 16);
    assert_eq!(games["rows"][0]["type"], "playoff");
    assert_eq!(games["truncated"], false);
    let all = tools
        .call(call("team_games", json!({ "season": 2016, "team": "CLE" })))
        .await
        .unwrap();
    assert!(all["games"].as_u64().unwrap() > 90);
    let extremes = tools
        .call(call(
            "rating_extremes",
            json!({ "metric": "peak", "order": "highest", "limit": 3 }),
        ))
        .await
        .unwrap();
    assert_eq!(extremes["team_seasons"].as_array().unwrap().len(), 3);
    assert_eq!(extremes["team_seasons"][0]["code"], "GSW");
    let p = tools
        .call(call(
            "win_probability",
            json!({ "elo_a": 1800, "elo_b": 1600, "home": "neutral" }),
        ))
        .await
        .unwrap();
    assert!(p["p_game_a"].as_f64().unwrap() > 0.7);
    // Mistakes come back as data for the model.
    for (name, args) in [
        ("season_standings", json!({ "season": 1900 })),
        ("season_standings", json!({ "season": 2010 })),
        ("team_games", json!({ "season": 2016, "team": "XXX" })),
        ("franchise_history", json!({ "franchise": "nobody" })),
        ("no_such_tool", json!({})),
    ] {
        let r = tools.call(call(name, args)).await.unwrap();
        assert!(r["error"].is_string(), "{name}: {r}");
    }

    // The whole use case with a scripted model: one lookup, then an answer.
    struct Script(Mutex<Vec<ChatReply>>);
    impl LanguageModel for Script {
        fn model_name(&self) -> &str {
            "script"
        }
        async fn chat(&self, _request: ChatRequest<'_>) -> Result<ChatReply, ModelError> {
            Ok(self.0.lock().unwrap().remove(0))
        }
    }
    let model = Script(Mutex::new(vec![
        ChatReply {
            tool_calls: vec![call("champions", json!({ "from": 2016, "to": 2016 }))],
            ..Default::default()
        },
        ChatReply {
            content: Some("Cleveland (CLE) won the 2015-16 title.".to_string()),
            ..Default::default()
        },
    ]));
    let today = NaiveDate::from_ymd_opt(2026, 10, 23).unwrap();
    let a = ask::answer(&store, &model, " who won in 2016? ", today, 100.0)
        .await
        .unwrap();
    assert!(!a.declined);
    assert_eq!(a.steps[0].summary, "1 row");
    // Arguments the adapter could not parse come back as an error the model can correct.
    let raw = tools
        .call(call("champions", json!({ "_raw": "{from: 2016" })))
        .await
        .unwrap();
    assert!(raw["error"].as_str().unwrap().contains("not valid JSON"));
    assert_eq!(a.question, "who won in 2016?");
    assert!(matches!(
        ask::answer(&store, &model, "   ", today, 100.0).await,
        Err(ask::AskError::Empty)
    ));
}

/// Game report links are kept by date and team codes with the lookup of their date.
#[sqlx::test(migrations = "./migrations")]
async fn game_reports_are_stored_with_their_lookup(pool: PgPool) {
    use nba_elo_simulator::application::ports::{GameReport, ReportStore};
    let (store, _teams) = common::setup(pool).await;
    let date = NaiveDate::from_ymd_opt(2016, 6, 19).unwrap();
    let other = NaiveDate::from_ymd_opt(2016, 6, 16).unwrap();
    assert!(store.reports_on(&[date, other]).await.unwrap().is_empty());
    assert!(store.lookups_on(&[date]).await.unwrap().is_empty());
    let report = GameReport {
        game_date: date,
        home_code: "GSW".into(),
        away_code: "CLE".into(),
        url: "https://www.espn.com/nba/recap/_/gameId/400878160".into(),
        title: Some("Cavaliers complete comeback".into()),
        source: "espn".into(),
    };
    store
        .save_lookup(date, "espn", std::slice::from_ref(&report))
        .await
        .unwrap();
    store.save_lookup(other, "espn", &[]).await.unwrap();
    let found = store.reports_on(&[date, other]).await.unwrap();
    assert_eq!(found, vec![report.clone()]);
    let lookups = store.lookups_on(&[date, other]).await.unwrap();
    assert_eq!(lookups.len(), 2);
    assert_eq!(
        lookups.iter().find(|l| l.game_date == date).unwrap().found,
        1
    );
    assert_eq!(
        lookups.iter().find(|l| l.game_date == other).unwrap().found,
        0
    );
    // A second lookup of the same date replaces the link, not duplicates it.
    let updated = GameReport {
        title: Some("Updated".into()),
        ..report
    };
    store
        .save_lookup(date, "espn", std::slice::from_ref(&updated))
        .await
        .unwrap();
    assert_eq!(store.reports_on(&[date]).await.unwrap(), vec![updated]);
}

// ---------------------------------------------------------------------------------------
// Players, box scores and injury reports
// ---------------------------------------------------------------------------------------

mod players {
    use super::*;
    use nba_elo_simulator::application::players;
    use nba_elo_simulator::application::ports::{
        BoxScoreLine, FetchProgressSink, InjuryRecord, PlayerRecord, PlayerSource, PlayerStore,
        SourceError,
    };
    use std::sync::Mutex;

    fn person(id: i64, first: &str, last: &str) -> PlayerRecord {
        PlayerRecord {
            source_id: id,
            first_name: first.to_string(),
            last_name: last.to_string(),
            position: Some("G".to_string()),
            height: Some("6-2".to_string()),
            weight: None,
            college: None,
            country: Some("USA".to_string()),
            draft_year: Some(2009),
            draft_round: Some(1),
            draft_number: Some(7),
            jersey_number: Some("30".to_string()),
            team_code: None,
        }
    }

    fn line(date: &str, team: &str, player: PlayerRecord, seconds: i32, pts: i32) -> BoxScoreLine {
        BoxScoreLine {
            date: super::date(date),
            team_code: team.to_string(),
            player,
            seconds: Some(seconds),
            pts: Some(pts),
            reb: Some(5),
            oreb: Some(1),
            dreb: Some(4),
            ast: Some(6),
            stl: Some(1),
            blk: Some(0),
            tov: Some(3),
            pf: Some(2),
            fgm: Some(10),
            fga: Some(20),
            fg3m: Some(4),
            fg3a: Some(10),
            ftm: Some(3),
            fta: Some(3),
            plus_minus: Some(pts - 20),
        }
    }

    /// Lines for one date only; every other date is empty. Fails once on the date given.
    struct FakePlayers {
        lines: Vec<BoxScoreLine>,
        fail_on: Mutex<Option<NaiveDate>>,
        asked: Mutex<Vec<NaiveDate>>,
        injuries: Vec<InjuryRecord>,
        players: Vec<PlayerRecord>,
        active: Vec<PlayerRecord>,
    }

    impl PlayerSource for FakePlayers {
        fn name(&self) -> &'static str {
            "fake-players"
        }
        async fn box_scores_on(
            &self,
            date: NaiveDate,
            progress: &dyn FetchProgressSink,
        ) -> Result<Vec<BoxScoreLine>, SourceError> {
            self.asked.lock().unwrap().push(date);
            if self
                .fail_on
                .lock()
                .unwrap()
                .take_if(|d| *d == date)
                .is_some()
            {
                return Err(SourceError::RateLimited(3));
            }
            progress.report(FetchProgress {
                requests: 1,
                ..Default::default()
            });
            Ok(self
                .lines
                .iter()
                .filter(|l| l.date == date)
                .cloned()
                .collect())
        }
        async fn injury_report(
            &self,
            _progress: &dyn FetchProgressSink,
        ) -> Result<Vec<InjuryRecord>, SourceError> {
            Ok(self.injuries.clone())
        }
        async fn all_players(
            &self,
            _progress: &dyn FetchProgressSink,
        ) -> Result<Vec<PlayerRecord>, SourceError> {
            Ok(self.players.clone())
        }
        async fn active_players(
            &self,
            _progress: &dyn FetchProgressSink,
        ) -> Result<Vec<PlayerRecord>, SourceError> {
            Ok(self.active.clone())
        }
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn box_scores_match_stored_games_and_the_backfill_resumes(pool: PgPool) {
        let (store, _teams) = common::setup(pool).await;
        imports::import_bytes(
            &store,
            common::FIXTURE.as_bytes(),
            prov("mini_2016.csv"),
            false,
        )
        .await
        .unwrap();
        // Opening night of the fixture: CHI hosts CLE, ATL hosts DET.
        let opening = "2015-10-27";
        let curry = person(115, "Stephen", "Curry");
        let lines = vec![
            line(opening, "CHI", person(1, "Derrick", "Rose"), 2052, 18),
            line(opening, "CLE", person(2, "LeBron", "James"), 2400, 25),
            line(opening, "CLE", person(3, "Kevin", "Love"), 1800, 10),
            // No stored game for this code on that date: counted, not stored.
            line(opening, "ZZZ", person(4, "Nobody", "Here"), 10, 0),
            line("2015-10-30", "GSW", curry.clone(), 2100, 40),
        ];
        let pending = store.dates_without_box_scores(S2016).await.unwrap();
        assert!(pending.len() > 100, "{} dates", pending.len());
        assert_eq!(pending[0], date(opening));
        assert!(pending.windows(2).all(|w| w[0] < w[1]), "dates in order");

        let fake = FakePlayers {
            lines,
            fail_on: Mutex::new(Some(pending[2])),
            asked: Mutex::new(vec![]),
            injuries: vec![],
            players: vec![],
            active: vec![],
        };
        // The first run stops at its third date; the two dates before it stay stored.
        let err = players::backfill_season(&store, &fake, S2016, &|_| {})
            .await
            .unwrap_err();
        assert_eq!(err.done.dates_fetched, 2);
        assert_eq!(err.dates_total, pending.len());
        assert_eq!(err.done.lines, 4);
        assert_eq!(err.done.matched, 3);
        assert_eq!(err.done.unmatched, 1);
        assert_eq!(err.done.players_upserted, 3);
        let left = store.dates_without_box_scores(S2016).await.unwrap();
        assert_eq!(left.len(), pending.len() - 2);
        assert_eq!(left[0], pending[2]);

        // The second run takes only the dates left and finishes.
        let outcome = players::backfill_season(&store, &fake, S2016, &|_| {})
            .await
            .unwrap();
        assert_eq!(outcome.dates_fetched, pending.len() - 2);
        assert_eq!(outcome.matched, 1, "Curry's line on 2015-10-30");
        let asked = fake.asked.lock().unwrap().clone();
        assert_eq!(
            asked.len(),
            pending.len() + 1,
            "the failed date was asked twice"
        );
        assert!(store
            .dates_without_box_scores(S2016)
            .await
            .unwrap()
            .is_empty());
        let fetches = store.box_score_fetches(S2016).await.unwrap();
        assert_eq!(fetches.len(), pending.len());
        assert_eq!(fetches[0].counts.lines, 4);
        assert_eq!(fetches[0].counts.unmatched, 1);

        // Players and their lines.
        let found = store.search_players("james", 10).await.unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].last_name, "James");
        assert!(
            store.search_players("%", 10).await.unwrap().is_empty(),
            "no wildcard"
        );
        let james = store.player(found[0].id).await.unwrap().unwrap();
        assert_eq!(james.source_id, 2);
        assert_eq!(james.draft_number, Some(7));
        let games = store.player_appearances(james.id, S2016).await.unwrap();
        assert_eq!(games.len(), 1);
        assert_eq!(games[0].team_code, "CLE");
        assert_eq!(games[0].opponent_code, "CHI");
        assert!(!games[0].home, "CLE visited CHI");
        assert_eq!(games[0].pts, Some(25));
        assert_eq!(games[0].oreb, Some(1));
        assert_eq!(games[0].pf, Some(2));
        let seasons = store.player_seasons(james.id).await.unwrap();
        assert_eq!(seasons.len(), 1);
        assert_eq!((seasons[0].games, seasons[0].pts), (1, Some(25)));
        let box_score = store.game_appearances(games[0].game_id).await.unwrap();
        assert_eq!(box_score.len(), 3, "three lines of the CHI–CLE game");
        assert!(box_score.iter().any(|l| l.team_code == "CHI" && l.home));
        assert!(store
            .player_appearances(james.id, SeasonYear::of(2017))
            .await
            .unwrap()
            .is_empty());

        // A second fetch of a date replaces its lines and updates the player.
        let mut again = person(2, "LeBron", "James");
        again.position = Some("F".to_string());
        let counts = store
            .save_box_scores(
                date(opening),
                "fake-players",
                &[line(opening, "CLE", again, 2000, 30)],
            )
            .await
            .unwrap();
        assert_eq!((counts.lines, counts.matched), (1, 1));
        let box_score = store.game_appearances(games[0].game_id).await.unwrap();
        assert_eq!(box_score.len(), 1);
        assert_eq!(box_score[0].pts, Some(30));
        assert_eq!(
            store.player(james.id).await.unwrap().unwrap().position,
            Some("F".to_string())
        );
        // A blank field never overwrites a filled one; an empty answer never erases a day.
        let mut blank = person(2, "", "");
        blank.position = None;
        blank.draft_number = None;
        store
            .save_box_scores(
                date(opening),
                "fake-players",
                &[line(opening, "CLE", blank, 1500, 12)],
            )
            .await
            .unwrap();
        let kept = store.player(james.id).await.unwrap().unwrap();
        assert_eq!(
            (kept.first_name.as_str(), kept.last_name.as_str()),
            ("LeBron", "James")
        );
        assert_eq!(
            (kept.position.as_deref(), kept.draft_number),
            (Some("F"), Some(7))
        );
        let counts = store
            .save_box_scores(date(opening), "fake-players", &[])
            .await
            .unwrap();
        assert_eq!(
            counts,
            nba_elo_simulator::application::ports::BoxScoreCounts::default()
        );
        assert_eq!(
            store
                .game_appearances(games[0].game_id)
                .await
                .unwrap()
                .len(),
            1
        );
        // A line dated another day than the fetch is not matched.
        let counts = store
            .save_box_scores(
                date("2015-10-28"),
                "fake-players",
                &[line(opening, "CLE", person(2, "LeBron", "James"), 1500, 12)],
            )
            .await
            .unwrap();
        assert_eq!((counts.matched, counts.unmatched), (0, 1));

        // The player list and the rosters: every player stored, the listed ones active
        // with their team, the rest inactive; a team the store does not know is left out.
        let mut durant = person(200, "Kevin", "Durant");
        durant.team_code = Some("PHO".to_string());
        let mut curry_now = curry.clone();
        curry_now.team_code = Some("GSW".to_string());
        let mut nobody = person(5, "Un", "Known");
        nobody.team_code = Some("ZZZ".to_string());
        let written = store
            .save_players(
                "fake-players",
                &[durant.clone(), curry_now.clone(), nobody.clone(), nobody],
            )
            .await
            .unwrap();
        assert_eq!(written, 3, "distinct players");
        let active = store
            .save_active_players("fake-players", &[curry_now, durant.clone()])
            .await
            .unwrap();
        assert_eq!(active, 2);
        let found = store.search_players("durant", 5).await.unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].team_code.as_deref(), Some("PHO"));
        assert!(found[0].active);
        assert_eq!(found[0].jersey_number.as_deref(), Some("30"));
        let unknown = store.search_players("un known", 5).await.unwrap();
        assert_eq!(
            unknown[0].team_code, None,
            "team code the store does not know"
        );
        assert!(!unknown[0].active);
        assert!(!store.player(james.id).await.unwrap().unwrap().active);
        // A second roster without Durant clears him; his team stays as last listed.
        let active = store
            .save_active_players("fake-players", std::slice::from_ref(&curry))
            .await
            .unwrap();
        assert_eq!(active, 1);
        let d = store.search_players("durant", 5).await.unwrap().remove(0);
        assert!(!d.active);
        assert_eq!(d.team_code.as_deref(), Some("PHO"));
        // An empty roster answer is refused, so a gap at the source clears nobody.
        let empty = FakePlayers {
            lines: vec![],
            fail_on: Mutex::new(None),
            asked: Mutex::new(vec![]),
            injuries: vec![],
            players: vec![],
            active: vec![],
        };
        assert!(players::refresh_active_players(&store, &empty, &|_| {})
            .await
            .is_err());
        assert!(store.search_players("curry", 5).await.unwrap()[0].active);
        assert_eq!(
            box_score[0].plus_minus,
            Some(10),
            "30 points: plus_minus = pts - 20"
        );

        let coverage = store.player_coverage().await.unwrap();
        assert_eq!(
            coverage.players, 6,
            "Rose, James, Love, Curry, Durant, Un Known"
        );
        assert_eq!(coverage.active_players, 1);
        assert_eq!(coverage.appearances, 2);
        assert_eq!(coverage.dates_fetched as usize, pending.len());
        assert_eq!(coverage.first_date_fetched, Some(date(opening)));
        assert_eq!(coverage.injury_report_dates, 0);
    }

    /// A run over every season takes the incomplete seasons newest first, resumes after
    /// a failure, and reports the per-season progress the Data page shows.
    #[sqlx::test(migrations = "./migrations")]
    async fn backfill_of_every_season_walks_the_incomplete_ones_and_resumes(pool: PgPool) {
        use nba_elo_simulator::application::continuation;
        let (store, _teams) = common::setup(pool).await;
        imports::import_bytes(
            &store,
            common::FIXTURE.as_bytes(),
            prov("mini_2016.csv"),
            false,
        )
        .await
        .unwrap();
        // A second, short season from a results source: three played dates.
        let s2017 = SeasonYear::of(2017);
        let results = FakeResults::with(vec![
            record("2016-10-25", "GSW", "SAS", Some((100, 129))),
            record("2016-10-28", "GSW", "NOP", Some((122, 114))),
            record("2016-10-30", "DAL", "GSW", Some((95, 100))),
        ]);
        continuation::sync_results(&store, &results, s2017, date("2016-10-31"), &|_| {})
            .await
            .unwrap();
        let progress = store.backfill_progress().await.unwrap();
        assert_eq!(progress.len(), 2);
        assert_eq!((progress[0].season, progress[0].dates_fetched), (S2016, 0));
        assert_eq!((progress[1].season, progress[1].dates_total), (s2017, 3));
        let dates_2016 = progress[0].dates_total;
        assert!(dates_2016 > 100);

        // The first run fails on the second date of 2016-17, the first season taken
        // (newest first): the run goes on with 2015-16 and completes it, then reports
        // the error.
        let fake = FakePlayers {
            lines: vec![line(
                "2016-10-28",
                "GSW",
                person(115, "Stephen", "Curry"),
                2100,
                40,
            )],
            fail_on: Mutex::new(Some(date("2016-10-28"))),
            asked: Mutex::new(vec![]),
            injuries: vec![],
            players: vec![],
            active: vec![],
        };
        let seen = Mutex::new(Vec::new());
        let err = players::backfill_all(&store, &fake, S2016, s2017, &|p| {
            seen.lock()
                .unwrap()
                .push((p.season, p.seasons_done, p.seasons_total));
        })
        .await
        .unwrap_err();
        assert_eq!(
            err.done.dates_fetched as i64,
            1 + dates_2016,
            "one date of 2016-17, then every date of 2015-16"
        );
        assert_eq!(
            err.done.matched, 0,
            "the matched line was on the failed date"
        );
        assert_eq!(err.dates_total as i64, 3 + dates_2016);
        assert!(err.to_string().contains("rate limit"), "{err}");
        let seen = seen.lock().unwrap().clone();
        assert_eq!(seen[0], (Some(s2017), 0, 2), "newest season first");
        assert!(
            seen.contains(&(Some(S2016), 1, 2)),
            "went on after the failure"
        );
        let progress = store.backfill_progress().await.unwrap();
        assert!(progress[0].is_complete(), "2015-16 complete");
        assert_eq!(progress[1].dates_fetched, 1);
        assert_eq!(
            players::seasons_to_backfill(&progress, S2016, s2017),
            vec![s2017]
        );
        // A season outside the range is left alone; the second run finishes 2016-17.
        let outcome = players::backfill_all(&store, &fake, S2016, S2016, &|_| {})
            .await
            .unwrap();
        assert_eq!(outcome.dates_fetched, 0);
        let outcome = players::backfill_all(&store, &fake, S2016, s2017, &|_| {})
            .await
            .unwrap();
        assert_eq!((outcome.dates_fetched, outcome.matched), (2, 1));
        assert!(store
            .backfill_progress()
            .await
            .unwrap()
            .iter()
            .all(|s| s.is_complete()));
        assert!(players::seasons_to_backfill(
            &store.backfill_progress().await.unwrap(),
            SeasonYear::MIN,
            SeasonYear::MAX
        )
        .is_empty());
    }

    /// The daily results sync replaces a season's non-archive games; the lines stored for
    /// those games must survive it, and a season removed and stored again is fetched again.
    #[sqlx::test(migrations = "./migrations")]
    async fn box_scores_survive_a_results_sync(pool: PgPool) {
        use nba_elo_simulator::application::continuation;
        let (store, _teams) = common::setup(pool).await;
        imports::import_bytes(
            &store,
            common::FIXTURE.as_bytes(),
            prov("mini_2016.csv"),
            false,
        )
        .await
        .unwrap();
        let s2017 = SeasonYear::of(2017);
        let results = |played: bool| {
            FakeResults::with(vec![
                record("2016-10-25", "GSW", "SAS", Some((100, 129))),
                record("2016-10-28", "GSW", "NOP", Some((122, 114))),
                record("2016-10-30", "DAL", "GSW", played.then_some((95, 100))),
            ])
        };
        continuation::sync_results(&store, &results(false), s2017, date("2016-10-31"), &|_| {})
            .await
            .unwrap();
        let day = "2016-10-28";
        let fake = FakePlayers {
            lines: vec![
                line(day, "GSW", person(115, "Stephen", "Curry"), 2100, 40),
                line(day, "NOP", person(9, "Anthony", "Davis"), 2200, 30),
            ],
            fail_on: Mutex::new(None),
            asked: Mutex::new(vec![]),
            injuries: vec![],
            players: vec![],
            active: vec![],
        };
        let outcome = players::backfill_season(&store, &fake, s2017, &|_| {})
            .await
            .unwrap();
        assert_eq!((outcome.dates_fetched, outcome.matched), (2, 2));
        let game_id = store.season_games(s2017).await.unwrap()[1].id;
        assert_eq!(store.game_appearances(game_id).await.unwrap().len(), 2);

        // The next day's sync brings the third game's result: the stored games are updated
        // in place and the lines stay attached.
        continuation::sync_results(&store, &results(true), s2017, date("2016-10-31"), &|_| {})
            .await
            .unwrap();
        let games = store.season_games(s2017).await.unwrap();
        assert_eq!(games[1].id, game_id, "the game keeps its identifier");
        assert!(games[2].is_played());
        assert_eq!(store.game_appearances(game_id).await.unwrap().len(), 2);
        assert_eq!(
            store.dates_without_box_scores(s2017).await.unwrap(),
            vec![date("2016-10-30")],
            "only the newly played date is unfetched"
        );
        assert_eq!(store.player_coverage().await.unwrap().appearances, 2);

        // A game the incoming data no longer holds goes, with its lines (a results upload
        // replaces the whole season; a sync keeps the games before its window).
        let csv = "date,home,away,home_score,away_score,neutral,playoff\n2016-10-25,GSW,SAS,100,129,0,0\n2016-10-30,DAL,GSW,95,100,0,0\n";
        continuation::import_results(&store, s2017, csv.as_bytes(), Some("results.csv"))
            .await
            .unwrap();
        assert_eq!(store.season_games(s2017).await.unwrap().len(), 2);
        assert!(store.game_appearances(game_id).await.unwrap().is_empty());
        assert_eq!(store.player_coverage().await.unwrap().appearances, 0);

        // Removing the season drops its lines and its fetch records, so a season stored
        // again starts from nothing.
        continuation::remove_season(&store, s2017).await.unwrap();
        let coverage = store.player_coverage().await.unwrap();
        assert_eq!((coverage.appearances, coverage.dates_fetched), (0, 0));
        continuation::sync_results(&store, &results(true), s2017, date("2016-10-31"), &|_| {})
            .await
            .unwrap();
        assert_eq!(
            store.dates_without_box_scores(s2017).await.unwrap().len(),
            3
        );
    }

    /// The season's lines feed the worth regression: a player who sat a block of games has
    /// a worth with an error; a line with no minutes and no stats is an absence; the
    /// injury report becomes penalties.
    #[sqlx::test(migrations = "./migrations")]
    async fn player_worth_and_availability_read_the_store(pool: PgPool) {
        use nba_elo_simulator::application::worth as app_worth;
        use nba_elo_simulator::domain::{bpm, worth};
        use std::collections::HashMap;
        let (store, teams) = common::setup(pool).await;
        imports::import_bytes(
            &store,
            common::FIXTURE.as_bytes(),
            prov("mini_2016.csv"),
            false,
        )
        .await
        .unwrap();
        // The Warriors' games of 2015-16 in order; a star on the box score of every game
        // except a block in the middle, where the team under-performed the ratings.
        let gsw = teams["GSW"].id;
        let mut games: Vec<_> = store
            .season_games(S2016)
            .await
            .unwrap()
            .into_iter()
            .filter(|g| g.is_played() && (g.home_team_id == gsw || g.away_team_id == gsw))
            .collect();
        games.sort_by_key(|g| (g.game_date, g.seq));
        assert!(games.len() > 80);
        let star = person(30, "Stephen", "Curry");
        let mut dates_saved = 0;
        for (i, g) in games.iter().enumerate() {
            let played = !(30..50).contains(&i);
            let date = g.game_date.to_string();
            let seconds = if played { 2100 } else { 0 };
            let mut l = line(&date, "GSW", star.clone(), seconds, 25);
            // Some DNP lines come with no minutes and no stats at all, as the source
            // writes them; they must read as absences, not fail the query.
            if (30..35).contains(&i) {
                l.seconds = None;
                l.pts = None;
                l.reb = None;
                l.ast = None;
                l.fga = None;
                l.fta = None;
            }
            store
                .save_box_scores(g.game_date, "fake-players", &[l])
                .await
                .unwrap();
            dates_saved += 1;
        }
        assert_eq!(dates_saved, games.len());
        let lines = store.season_lines(S2016).await.unwrap();
        assert_eq!(lines.len(), games.len());
        assert_eq!(
            lines.iter().filter(|l| !l.played).count(),
            20,
            "the DNP lines"
        );
        // The full lines for the box-score worth: every statistic comes back as stored,
        // and a null stays null.
        let full = store.season_box_lines(S2016).await.unwrap();
        assert_eq!(full.len(), games.len());
        let played_line = full.iter().find(|l| l.seconds == Some(2100)).unwrap();
        assert_eq!(
            (
                played_line.pts,
                played_line.oreb,
                played_line.stl,
                played_line.fga
            ),
            (Some(25), Some(1), Some(1), Some(20))
        );
        assert_eq!(full.iter().filter(|l| l.seconds.is_none()).count(), 5);
        assert!(full.iter().all(|l| l.team_id == gsw));
        // One team's lines only: no game is covered, and the table says why.
        let box_table = bpm::table(S2016, &games, &full, &HashMap::new());
        assert_eq!(box_table.games, 0);
        assert!(box_table.players.is_empty());
        assert!(box_table.note.as_deref().unwrap().contains("both teams"));
        let t = worth::table(
            S2016,
            &store.season_games(S2016).await.unwrap(),
            &lines,
            worth::WorthParams::default(),
        );
        assert_eq!(t.players.len(), 1);
        let w = &t.players[0];
        assert_eq!((w.absences, w.teams.as_slice()), (20, &[gsw][..]));
        assert_eq!(w.games_played as usize, games.len() - 20);
        assert!(w.se_points > 0.0 && w.se_points < 5.0, "{}", w.se_points);
        assert!(!t.prior_estimated, "one player: the default prior");
        // The names join: the store's player row carries the name.
        let found = store.players_by_id(&[w.player_id]).await.unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].last_name, "Curry");
        assert!(store.players_by_id(&[]).await.unwrap().is_empty());
        // The injury report priced: Out with a return date, for the Warriors.
        let worth_elo = w.worth_elo;
        let reported = vec![worth::ReportedAbsence {
            player_id: w.player_id,
            team_id: gsw,
            status: worth::AbsenceStatus::Out,
            return_date: Some(date("2016-01-10")),
            worth_elo,
        }];
        let absences =
            worth::absences_from_report(&reported, date("2016-01-05"), date("2016-06-20"));
        if worth_elo > 0.0 {
            assert_eq!(absences.len(), 1);
            assert_eq!(
                (absences[0].from, absences[0].to),
                (date("2016-01-05"), date("2016-01-09"))
            );
        } else {
            assert!(absences.is_empty(), "no reward for an absence");
        }
        let _ = app_worth::WORTH_LOOKBACK;
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn injury_reports_are_kept_day_by_day(pool: PgPool) {
        let (store, _teams) = common::setup(pool).await;
        let report = |status: &str| InjuryRecord {
            player: person(115, "Stephen", "Curry"),
            team_code: "GSW".to_string(),
            status: status.to_string(),
            return_date: Some(date("2026-01-20")),
            description: Some("Knee; sprain".to_string()),
        };
        let fake = FakePlayers {
            lines: vec![],
            fail_on: Mutex::new(None),
            asked: Mutex::new(vec![]),
            injuries: vec![
                report("Out"),
                InjuryRecord {
                    player: person(7, "Kevon", "Looney"),
                    team_code: "GSW".to_string(),
                    status: "Day-To-Day".to_string(),
                    return_date: None,
                    description: None,
                },
            ],
            players: vec![],
            active: vec![],
        };
        assert_eq!(store.latest_injury_report_date().await.unwrap(), None);
        let day1 = date("2026-01-15");
        let stored = players::fetch_injury_report(&store, &fake, day1, &|_| {})
            .await
            .unwrap();
        assert_eq!(stored, 2);
        // The next day's report replaces nothing of the day before.
        let day2 = date("2026-01-16");
        store
            .save_injury_report(day2, "fake-players", &[report("Questionable")])
            .await
            .unwrap();
        assert_eq!(store.latest_injury_report_date().await.unwrap(), Some(day2));
        let first = store.injury_report(day1).await.unwrap();
        assert_eq!(first.len(), 2);
        let curry = first
            .iter()
            .find(|l| l.player_name == "Stephen Curry")
            .unwrap();
        assert_eq!(curry.status, "Out");
        assert_eq!(curry.team_code, "GSW");
        assert_eq!(curry.return_date, Some(date("2026-01-20")));
        let second = store.injury_report(day2).await.unwrap();
        assert_eq!(second.len(), 1);
        assert_eq!(second[0].status, "Questionable");
        assert_eq!(
            second[0].player_id, curry.player_id,
            "the same stored player"
        );
        // Saving a day again replaces that day's rows.
        store
            .save_injury_report(day2, "fake-players", &[])
            .await
            .unwrap();
        assert!(store.injury_report(day2).await.unwrap().is_empty());
        assert_eq!(store.injury_report(day1).await.unwrap().len(), 2);
        // A line for a team code the store does not know is dropped, not an error.
        let n = store
            .save_injury_report(
                day2,
                "fake-players",
                &[InjuryRecord {
                    team_code: "ZZZ".to_string(),
                    ..report("Out")
                }],
            )
            .await
            .unwrap();
        assert_eq!(n, 0);
        let coverage = store.player_coverage().await.unwrap();
        assert_eq!(coverage.injury_report_dates, 1, "day 2 holds no rows now");
        assert_eq!(coverage.last_injury_report, Some(day1));
    }
}
