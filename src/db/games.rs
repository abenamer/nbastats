use chrono::NaiveDate;

use crate::models::{Game, Season};

use super::Pool;

const GAME_COLS: &str = "id, season, game_date, seq, home_team_id, away_team_id, neutral, game_type, home_score, away_score,
    home_elo_pre, away_elo_pre, home_elo_post, away_elo_post, home_win_prob";

pub async fn season_games(pool: &Pool, season: i32) -> anyhow::Result<Vec<Game>> {
    Ok(sqlx::query_as::<_, Game>(&format!(
        "SELECT {GAME_COLS} FROM games WHERE season = $1 ORDER BY game_date, seq, id"
    ))
    .bind(season)
    .fetch_all(pool)
    .await?)
}

/// Games of a season on or before `cutoff` (inclusive). Used to build leakage-free forecasts.
pub async fn season_games_until(pool: &Pool, season: i32, cutoff: NaiveDate) -> anyhow::Result<Vec<Game>> {
    Ok(sqlx::query_as::<_, Game>(&format!(
        "SELECT {GAME_COLS} FROM games WHERE season = $1 AND game_date <= $2 ORDER BY game_date, seq, id"
    ))
    .bind(season)
    .bind(cutoff)
    .fetch_all(pool)
    .await?)
}

pub async fn team_games(pool: &Pool, team_id: i32, season: i32) -> anyhow::Result<Vec<Game>> {
    Ok(sqlx::query_as::<_, Game>(&format!(
        "SELECT {GAME_COLS} FROM games WHERE season = $1 AND (home_team_id = $2 OR away_team_id = $2) ORDER BY game_date, seq, id"
    ))
    .bind(season)
    .bind(team_id)
    .fetch_all(pool)
    .await?)
}

pub async fn seasons(pool: &Pool) -> anyhow::Result<Vec<Season>> {
    Ok(sqlx::query_as::<_, Season>(
        "SELECT season, label, team_count, game_count, playoff_game_count, play_in_game_count, cup_final_game_count, regular_season_end,
                playoffs_start, playoffs_end, complete FROM seasons ORDER BY season",
    )
    .fetch_all(pool)
    .await?)
}

pub async fn season(pool: &Pool, season: i32) -> anyhow::Result<Option<Season>> {
    Ok(sqlx::query_as::<_, Season>(
        "SELECT season, label, team_count, game_count, playoff_game_count, play_in_game_count, cup_final_game_count, regular_season_end,
                playoffs_start, playoffs_end, complete FROM seasons WHERE season = $1",
    )
    .bind(season)
    .fetch_optional(pool)
    .await?)
}

pub async fn latest_season(pool: &Pool) -> anyhow::Result<Option<i32>> {
    let r: Option<(i32,)> = sqlx::query_as("SELECT max(season) FROM seasons").fetch_optional(pool).await?;
    Ok(r.map(|x| x.0))
}

pub async fn game_count(pool: &Pool) -> anyhow::Result<i64> {
    let r: (i64,) = sqlx::query_as("SELECT count(*) FROM games").fetch_one(pool).await?;
    Ok(r.0)
}

/// Elo trajectory of a franchise: one point per game, across all its team codes.
#[derive(Debug, Clone, serde::Serialize, sqlx::FromRow)]
pub struct EloPoint {
    pub season: i32,
    pub game_date: NaiveDate,
    pub code: String,
    pub game_type: String,
    pub elo_post: f64,
}

pub async fn franchise_elo_history(pool: &Pool, franchise_id: i32, from: i32, to: i32) -> anyhow::Result<Vec<EloPoint>> {
    Ok(sqlx::query_as::<_, EloPoint>(
        "SELECT g.season, g.game_date, t.code, g.game_type,
                CASE WHEN g.home_team_id = t.id THEN g.home_elo_post ELSE g.away_elo_post END AS elo_post
         FROM games g JOIN teams t ON t.id IN (g.home_team_id, g.away_team_id)
         WHERE t.franchise_id = $1 AND g.season BETWEEN $2 AND $3
         ORDER BY g.game_date, g.seq, g.id",
    )
    .bind(franchise_id)
    .bind(from)
    .bind(to)
    .fetch_all(pool)
    .await?)
}
