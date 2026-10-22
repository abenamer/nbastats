//! `AnalysisStore` for PostgreSQL: derived tables, forecasts and health.

use std::collections::HashMap;

use chrono::NaiveDate;

use crate::application::ports::{AnalysisStore, NewForecast, StoreHealth, StoreResult};
use crate::domain::ids::{FranchiseId, SeasonYear};
use crate::domain::models::{ChampionshipView, TeamSeasonView};
use crate::domain::records::{GameSwing, TeamSeasonStat};

use super::PostgresStore;

const TS_VIEW_COLS: &str = "ts.season, ts.team_id, t.code, t.name AS team_name, ts.franchise_id, f.key AS franchise_key,
    f.name AS franchise_name, ts.conference, ts.division, ts.wins, ts.losses, ts.win_pct, ts.elo_start, ts.elo_regular_end,
    ts.elo_pre_playoffs, ts.elo_end, ts.elo_peak, ts.made_play_in, ts.made_playoffs, ts.seed, ts.playoff_result";

impl AnalysisStore for PostgresStore {
    async fn team_seasons(&self, season: SeasonYear) -> StoreResult<Vec<TeamSeasonView>> {
        Ok(sqlx::query_as::<_, TeamSeasonView>(&format!(
            "SELECT {TS_VIEW_COLS} FROM team_seasons ts JOIN teams t ON t.id = ts.team_id JOIN franchises f ON f.id = ts.franchise_id
             WHERE ts.season = $1 ORDER BY ts.conference NULLS LAST, ts.win_pct DESC, t.code"
        ))
        .bind(season)
        .fetch_all(&self.pool)
        .await?)
    }

    async fn franchise_seasons(
        &self,
        franchise: FranchiseId,
        from: SeasonYear,
        to: SeasonYear,
    ) -> StoreResult<Vec<TeamSeasonView>> {
        Ok(sqlx::query_as::<_, TeamSeasonView>(&format!(
            "SELECT {TS_VIEW_COLS} FROM team_seasons ts JOIN teams t ON t.id = ts.team_id JOIN franchises f ON f.id = ts.franchise_id
             WHERE ts.franchise_id = $1 AND ts.season BETWEEN $2 AND $3 ORDER BY ts.season"
        ))
        .bind(franchise)
        .bind(from)
        .bind(to)
        .fetch_all(&self.pool)
        .await?)
    }

    async fn championships(
        &self,
        from: SeasonYear,
        to: SeasonYear,
    ) -> StoreResult<Vec<ChampionshipView>> {
        Ok(sqlx::query_as::<_, ChampionshipView>(
            "SELECT c.season, t.code AS champion_code, t.name AS champion_name, f.key AS franchise_key, f.name AS franchise_name,
                    r.code AS runner_up_code, c.source
             FROM championships c JOIN teams t ON t.id = c.champion_team_id JOIN franchises f ON f.id = c.champion_franchise_id
             LEFT JOIN teams r ON r.id = c.runner_up_team_id
             WHERE c.season BETWEEN $1 AND $2 ORDER BY c.season",
        )
        .bind(from)
        .bind(to)
        .fetch_all(&self.pool)
        .await?)
    }

    async fn title_counts(
        &self,
        from: SeasonYear,
        to: SeasonYear,
    ) -> StoreResult<HashMap<String, i64>> {
        let rows: Vec<(String, i64)> = sqlx::query_as(
            "SELECT f.key, count(*) FROM championships c JOIN franchises f ON f.id = c.champion_franchise_id
             WHERE c.season BETWEEN $1 AND $2 GROUP BY f.key",
        )
        .bind(from)
        .bind(to)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().collect())
    }

    async fn complete_playoff_seasons(
        &self,
        from: SeasonYear,
        to: SeasonYear,
    ) -> StoreResult<Vec<SeasonYear>> {
        let rows: Vec<(SeasonYear,)> =
            sqlx::query_as("SELECT season FROM seasons WHERE complete AND season BETWEEN $1 AND $2 ORDER BY season")
                .bind(from)
                .bind(to)
                .fetch_all(&self.pool)
                .await?;
        Ok(rows.into_iter().map(|r| r.0).collect())
    }

    async fn team_season_stats(&self) -> StoreResult<Vec<TeamSeasonStat>> {
        Ok(sqlx::query_as::<_, TeamSeasonStat>(
            "WITH p AS (
                 SELECT g.season, t.id AS team_id, g.game_date,
                        CASE WHEN g.home_team_id = t.id THEN g.home_elo_post ELSE g.away_elo_post END AS elo_post
                 FROM games g JOIN teams t ON t.id IN (g.home_team_id, g.away_team_id)
                 WHERE g.home_score IS NOT NULL
             ), a AS (
                 SELECT season, team_id, count(*) AS games, max(elo_post) AS peak, min(elo_post) AS low,
                        (array_agg(game_date ORDER BY elo_post DESC, game_date))[1] AS peak_date,
                        (array_agg(game_date ORDER BY elo_post ASC, game_date))[1] AS low_date
                 FROM p GROUP BY season, team_id
             )
             SELECT a.season, t.code, t.name, f.key AS franchise_key, a.games, a.peak, a.peak_date, a.low, a.low_date,
                    ts.wins, ts.losses, ts.elo_start, ts.elo_pre_playoffs, ts.elo_end, ts.playoff_result,
                    ts.conference, ts.division
             FROM a JOIN teams t ON t.id = a.team_id JOIN franchises f ON f.id = t.franchise_id
             LEFT JOIN team_seasons ts ON ts.season = a.season AND ts.team_id = a.team_id
             ORDER BY a.season, t.code",
        )
        .fetch_all(&self.pool)
        .await?)
    }

    async fn biggest_swings(&self, limit: i64) -> StoreResult<Vec<GameSwing>> {
        type Row = (
            SeasonYear,
            NaiveDate,
            String,
            String,
            String,
            i32,
            i32,
            bool,
            bool,
            f64,
            f64,
            f64,
        );
        let rows: Vec<Row> = sqlx::query_as(
            "SELECT * FROM (
                 SELECT g.season, g.game_date, w.code, w.name, l.code AS opponent_code,
                        CASE WHEN g.home_score > g.away_score THEN g.home_score ELSE g.away_score END AS points,
                        CASE WHEN g.home_score > g.away_score THEN g.away_score ELSE g.home_score END AS opp_points,
                        g.home_score > g.away_score AS home,
                        g.game_type = 'playoff' AS playoff,
                        CASE WHEN g.home_score > g.away_score THEN g.home_elo_pre ELSE g.away_elo_pre END AS elo_pre,
                        CASE WHEN g.home_score > g.away_score THEN g.home_elo_post ELSE g.away_elo_post END AS elo_post,
                        CASE WHEN g.home_score > g.away_score THEN g.away_elo_pre ELSE g.home_elo_pre END AS opp_elo_pre
                 FROM games g
                 JOIN teams w ON w.id = CASE WHEN g.home_score > g.away_score THEN g.home_team_id ELSE g.away_team_id END
                 JOIN teams l ON l.id = CASE WHEN g.home_score > g.away_score THEN g.away_team_id ELSE g.home_team_id END
                 WHERE g.home_score IS NOT NULL AND g.home_score <> g.away_score
             ) s
             ORDER BY s.elo_post - s.elo_pre DESC, s.game_date
             LIMIT $1",
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(
                |(
                    season,
                    game_date,
                    code,
                    name,
                    opponent_code,
                    points,
                    opponent_points,
                    home,
                    playoff,
                    elo_pre,
                    elo_post,
                    opponent_elo_pre,
                )| GameSwing {
                    season,
                    game_date,
                    code,
                    name,
                    opponent_code,
                    points,
                    opponent_points,
                    home,
                    playoff,
                    elo_pre,
                    elo_post,
                    opponent_elo_pre,
                },
            )
            .collect())
    }

    async fn season_rosters(&self) -> StoreResult<Vec<(SeasonYear, String)>> {
        Ok(sqlx::query_as(
            "SELECT DISTINCT g.season, t.code
             FROM games g JOIN teams t ON t.id IN (g.home_team_id, g.away_team_id)
             ORDER BY g.season, t.code",
        )
        .fetch_all(&self.pool)
        .await?)
    }

    async fn insert_forecast(&self, f: &NewForecast<'_>) -> StoreResult<()> {
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
        .bind(f.team)
        .bind(f.seed)
        .bind(f.probs[0])
        .bind(f.probs[1])
        .bind(f.probs[2])
        .bind(f.probs[3])
        .bind(f.actual_result)
        .bind(f.actual_champion)
        .bind(f.cutoff)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn table_counts(&self) -> StoreResult<Vec<(String, i64)>> {
        let mut out = Vec::new();
        for t in [
            "franchises",
            "teams",
            "seasons",
            "games",
            "team_seasons",
            "championships",
            "data_imports",
            "simulation_jobs",
            "forecasts",
        ] {
            let r: (i64,) = sqlx::query_as(&format!("SELECT count(*) FROM {t}"))
                .fetch_one(&self.pool)
                .await?;
            out.push((t.to_string(), r.0));
        }
        Ok(out)
    }

    async fn health(&self) -> StoreHealth {
        self.health_check().await
    }
}
