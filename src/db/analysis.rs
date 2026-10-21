//! Queries over the derived tables (team_seasons, championships, forecasts).

use std::collections::HashMap;

use crate::models::{ChampionshipView, PlayoffResult, TeamSeason, TeamSeasonView};

use super::Pool;

const TS_VIEW_COLS: &str = "ts.season, ts.team_id, t.code, t.name AS team_name, ts.franchise_id, f.key AS franchise_key,
    f.name AS franchise_name, ts.conference, ts.division, ts.wins, ts.losses, ts.win_pct, ts.elo_start, ts.elo_regular_end,
    ts.elo_pre_playoffs, ts.elo_end, ts.elo_peak, ts.made_play_in, ts.made_playoffs, ts.seed, ts.playoff_result";

pub async fn team_seasons(pool: &Pool, season: i32) -> anyhow::Result<Vec<TeamSeasonView>> {
    Ok(sqlx::query_as::<_, TeamSeasonView>(&format!(
        "SELECT {TS_VIEW_COLS} FROM team_seasons ts JOIN teams t ON t.id = ts.team_id JOIN franchises f ON f.id = ts.franchise_id
         WHERE ts.season = $1 ORDER BY ts.conference NULLS LAST, ts.win_pct DESC, t.code"
    ))
    .bind(season)
    .fetch_all(pool)
    .await?)
}

pub async fn franchise_seasons(pool: &Pool, franchise_id: i32, from: i32, to: i32) -> anyhow::Result<Vec<TeamSeasonView>> {
    Ok(sqlx::query_as::<_, TeamSeasonView>(&format!(
        "SELECT {TS_VIEW_COLS} FROM team_seasons ts JOIN teams t ON t.id = ts.team_id JOIN franchises f ON f.id = ts.franchise_id
         WHERE ts.franchise_id = $1 AND ts.season BETWEEN $2 AND $3 ORDER BY ts.season"
    ))
    .bind(franchise_id)
    .bind(from)
    .bind(to)
    .fetch_all(pool)
    .await?)
}

pub async fn upsert_team_season(pool: &Pool, ts: &TeamSeason) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO team_seasons (season, team_id, franchise_id, conference, division, wins, losses, win_pct, elo_start,
            elo_regular_end, elo_pre_playoffs, elo_end, elo_peak, made_play_in, made_playoffs, seed, playoff_result)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17)
         ON CONFLICT (season, team_id) DO UPDATE SET franchise_id = EXCLUDED.franchise_id, conference = EXCLUDED.conference,
            division = EXCLUDED.division, wins = EXCLUDED.wins, losses = EXCLUDED.losses, win_pct = EXCLUDED.win_pct,
            elo_start = EXCLUDED.elo_start, elo_regular_end = EXCLUDED.elo_regular_end, elo_pre_playoffs = EXCLUDED.elo_pre_playoffs,
            elo_end = EXCLUDED.elo_end, elo_peak = EXCLUDED.elo_peak, made_play_in = EXCLUDED.made_play_in,
            made_playoffs = EXCLUDED.made_playoffs, seed = EXCLUDED.seed, playoff_result = EXCLUDED.playoff_result",
    )
    .bind(ts.season)
    .bind(ts.team_id)
    .bind(ts.franchise_id)
    .bind(ts.conference)
    .bind(&ts.division)
    .bind(ts.wins)
    .bind(ts.losses)
    .bind(ts.win_pct)
    .bind(ts.elo_start)
    .bind(ts.elo_regular_end)
    .bind(ts.elo_pre_playoffs)
    .bind(ts.elo_end)
    .bind(ts.elo_peak)
    .bind(ts.made_play_in)
    .bind(ts.made_playoffs)
    .bind(ts.seed)
    .bind(ts.playoff_result)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn upsert_championship(
    pool: &Pool,
    season: i32,
    champion_team_id: i32,
    champion_franchise_id: i32,
    runner_up_team_id: Option<i32>,
    source: &str,
) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO championships (season, champion_team_id, champion_franchise_id, runner_up_team_id, source)
         VALUES ($1,$2,$3,$4,$5)
         ON CONFLICT (season) DO UPDATE SET champion_team_id = EXCLUDED.champion_team_id,
            champion_franchise_id = EXCLUDED.champion_franchise_id, runner_up_team_id = EXCLUDED.runner_up_team_id,
            source = EXCLUDED.source, updated_at = now()",
    )
    .bind(season)
    .bind(champion_team_id)
    .bind(champion_franchise_id)
    .bind(runner_up_team_id)
    .bind(source)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn championships(pool: &Pool, from: i32, to: i32) -> anyhow::Result<Vec<ChampionshipView>> {
    Ok(sqlx::query_as::<_, ChampionshipView>(
        "SELECT c.season, t.code AS champion_code, t.name AS champion_name, f.key AS franchise_key, f.name AS franchise_name,
                r.code AS runner_up_code, c.source
         FROM championships c JOIN teams t ON t.id = c.champion_team_id JOIN franchises f ON f.id = c.champion_franchise_id
         LEFT JOIN teams r ON r.id = c.runner_up_team_id
         WHERE c.season BETWEEN $1 AND $2 ORDER BY c.season",
    )
    .bind(from)
    .bind(to)
    .fetch_all(pool)
    .await?)
}

/// Title counts per franchise key within a season range.
pub async fn title_counts(pool: &Pool, from: i32, to: i32) -> anyhow::Result<HashMap<String, i64>> {
    let rows: Vec<(String, i64)> = sqlx::query_as(
        "SELECT f.key, count(*) FROM championships c JOIN franchises f ON f.id = c.champion_franchise_id
         WHERE c.season BETWEEN $1 AND $2 GROUP BY f.key",
    )
    .bind(from)
    .bind(to)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().collect())
}

/// Seasons that have a complete, reconstructable 16-team playoff (used by backtests).
pub async fn complete_playoff_seasons(pool: &Pool, from: i32, to: i32) -> anyhow::Result<Vec<i32>> {
    let rows: Vec<(i32,)> =
        sqlx::query_as("SELECT season FROM seasons WHERE complete AND season BETWEEN $1 AND $2 ORDER BY season")
            .bind(from)
            .bind(to)
            .fetch_all(pool)
            .await?;
    Ok(rows.into_iter().map(|r| r.0).collect())
}

/// One pre-playoff forecast row to persist.
pub struct NewForecast<'a> {
    pub job_id: uuid::Uuid,
    pub model: &'a str,
    pub season: i32,
    pub team_id: i32,
    pub seed: Option<i32>,
    /// [p_second_round, p_conf_finals, p_finals, p_champion]
    pub probs: [f64; 4],
    pub actual_result: PlayoffResult,
    pub actual_champion: bool,
    pub cutoff: chrono::NaiveDate,
}

pub async fn insert_forecast(pool: &Pool, f: &NewForecast<'_>) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO forecasts (job_id, model, season, team_id, seed, p_second_round, p_conf_finals, p_finals, p_champion,
            actual_result, actual_champion, information_cutoff)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)
         ON CONFLICT (job_id, model, season, team_id) DO UPDATE SET p_second_round = EXCLUDED.p_second_round,
            p_conf_finals = EXCLUDED.p_conf_finals, p_finals = EXCLUDED.p_finals, p_champion = EXCLUDED.p_champion,
            actual_result = EXCLUDED.actual_result, actual_champion = EXCLUDED.actual_champion,
            information_cutoff = EXCLUDED.information_cutoff",
    )
    .bind(f.job_id)
    .bind(f.model)
    .bind(f.season)
    .bind(f.team_id)
    .bind(f.seed)
    .bind(f.probs[0])
    .bind(f.probs[1])
    .bind(f.probs[2])
    .bind(f.probs[3])
    .bind(f.actual_result)
    .bind(f.actual_champion)
    .bind(f.cutoff)
    .execute(pool)
    .await?;
    Ok(())
}

#[derive(Debug, Clone, serde::Serialize, sqlx::FromRow)]
pub struct ForecastView {
    pub model: String,
    pub season: i32,
    pub code: String,
    pub franchise_key: String,
    pub seed: Option<i32>,
    pub p_second_round: f64,
    pub p_conf_finals: f64,
    pub p_finals: f64,
    pub p_champion: f64,
    pub actual_result: PlayoffResult,
    pub actual_champion: bool,
    pub information_cutoff: chrono::NaiveDate,
}

pub async fn forecasts_for_job(pool: &Pool, job_id: uuid::Uuid) -> anyhow::Result<Vec<ForecastView>> {
    Ok(sqlx::query_as::<_, ForecastView>(
        "SELECT fc.model, fc.season, t.code, f.key AS franchise_key, fc.seed, fc.p_second_round, fc.p_conf_finals,
                fc.p_finals, fc.p_champion, fc.actual_result, fc.actual_champion, fc.information_cutoff
         FROM forecasts fc JOIN teams t ON t.id = fc.team_id JOIN franchises f ON f.id = t.franchise_id
         WHERE fc.job_id = $1 ORDER BY fc.model, fc.season, fc.p_champion DESC",
    )
    .bind(job_id)
    .fetch_all(pool)
    .await?)
}

/// Row counts of the main tables for the data management page.
pub async fn table_counts(pool: &Pool) -> anyhow::Result<Vec<(String, i64)>> {
    let mut out = Vec::new();
    for t in ["franchises", "teams", "seasons", "games", "team_seasons", "championships", "data_imports", "simulation_jobs", "forecasts"] {
        let r: (i64,) = sqlx::query_as(&format!("SELECT count(*) FROM {t}")).fetch_one(pool).await?;
        out.push((t.to_string(), r.0));
    }
    Ok(out)
}
