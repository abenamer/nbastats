//! `AnalysisStore` for PostgreSQL: derived tables, forecasts and health.

use std::collections::HashMap;

use chrono::NaiveDate;

use crate::application::ports::{AnalysisStore, NewForecast, StoreHealth, StoreResult};
use crate::domain::ids::{FranchiseId, SeasonYear};
use crate::domain::models::{ChampionshipView, TeamSeasonView};
use crate::domain::records::{ChampionRoad, GameSwing, Matchup, MatchupOrder, TeamSeasonStat};
use crate::domain::reference;

use super::PostgresStore;

const TS_VIEW_COLS: &str = "ts.season, ts.team_id, t.code, t.name AS team_name, ts.franchise_id, f.key AS franchise_key,
    f.name AS franchise_name, ts.conference, ts.division, ts.wins, ts.losses, ts.win_pct, ts.elo_start, ts.elo_regular_end,
    ts.elo_pre_playoffs, ts.elo_end, ts.elo_peak, ts.made_play_in, ts.made_playoffs, ts.seed, ts.playoff_result";

/// The matchup query with its sort direction, which cannot be a bound parameter.
macro_rules! matchups_sql {
    ($direction:literal) => {
        concat!(
            "SELECT g.season, g.game_date, h.code, h.name, a.code, a.name, ",
            "g.home_elo_pre, g.away_elo_pre, g.home_score, g.away_score, g.game_type = 'playoff' AS playoff ",
            "FROM games g JOIN teams h ON h.id = g.home_team_id JOIN teams a ON a.id = g.away_team_id ",
            "WHERE g.home_score IS NOT NULL ",
            "ORDER BY g.home_elo_pre + g.away_elo_pre ",
            $direction,
            ", g.game_date LIMIT $1"
        )
    };
}
const MATCHUPS_DESC: &str = matchups_sql!("DESC");

const MATCHUPS_ASC: &str = matchups_sql!("ASC");

type MatchupRow = (
    SeasonYear,
    NaiveDate,
    String,
    String,
    String,
    String,
    f64,
    f64,
    i32,
    i32,
    bool,
);

fn matchup_from_row(
    (
        season,
        game_date,
        home_code,
        home_name,
        away_code,
        away_name,
        home_elo_pre,
        away_elo_pre,
        home_score,
        away_score,
        playoff,
    ): MatchupRow,
) -> Matchup {
    Matchup {
        season,
        game_date,
        home_code,
        home_name,
        away_code,
        away_name,
        home_elo_pre,
        away_elo_pre,
        home_score,
        away_score,
        playoff,
    }
}

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
                    ts.wins, ts.losses, ts.elo_start, ts.elo_pre_playoffs, ts.elo_end,
                    CASE WHEN c.champion_team_id = a.team_id THEN 'champion'
                         WHEN c.season IS NOT NULL AND ts.playoff_result = 'champion' THEN 'finals'
                         ELSE ts.playoff_result END AS playoff_result,
                    ts.conference, ts.division
             FROM a JOIN teams t ON t.id = a.team_id JOIN franchises f ON f.id = t.franchise_id
             LEFT JOIN team_seasons ts ON ts.season = a.season AND ts.team_id = a.team_id
             LEFT JOIN championships c ON c.season = a.season
             ORDER BY a.season, t.code",
        )
        .fetch_all(&self.pool)
        .await?)
    }

    async fn champion_roads(&self) -> StoreResult<Vec<ChampionRoad>> {
        type Row = (
            SeasonYear,
            String,
            String,
            Option<f64>,
            i64,
            Option<f64>,
            i64,
            Option<String>,
            Option<f64>,
        );
        let rows: Vec<Row> = sqlx::query_as(
            "WITH o AS (
                 SELECT g.season, c.champion_team_id AS team_id, g.game_type,
                        CASE WHEN g.home_team_id = c.champion_team_id THEN g.away_elo_pre ELSE g.home_elo_pre END AS opp_elo,
                        CASE WHEN g.home_team_id = c.champion_team_id THEN g.away_team_id ELSE g.home_team_id END AS opp_id
                 FROM championships c
                 JOIN games g ON g.season = c.season AND c.champion_team_id IN (g.home_team_id, g.away_team_id)
                 WHERE g.home_score IS NOT NULL AND g.game_type <> 'cup_final'
             )
             SELECT o.season, t.code, t.name,
                    avg(o.opp_elo) FILTER (WHERE o.game_type = 'regular'),
                    count(*) FILTER (WHERE o.game_type = 'regular'),
                    avg(o.opp_elo) FILTER (WHERE o.game_type <> 'regular'),
                    count(*) FILTER (WHERE o.game_type <> 'regular'),
                    (array_agg(p.code ORDER BY o.opp_elo DESC) FILTER (WHERE o.game_type <> 'regular'))[1],
                    max(o.opp_elo) FILTER (WHERE o.game_type <> 'regular')
             FROM o JOIN teams t ON t.id = o.team_id JOIN teams p ON p.id = o.opp_id
             GROUP BY o.season, t.code, t.name
             ORDER BY o.season",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(
                |(
                    season,
                    code,
                    name,
                    regular,
                    regular_games,
                    playoff,
                    playoff_games,
                    toughest_code,
                    toughest_elo,
                )| ChampionRoad {
                    season,
                    code,
                    name,
                    regular_opponent_elo: regular.unwrap_or(0.0),
                    regular_games,
                    playoff_opponent_elo: playoff.unwrap_or(0.0),
                    playoff_games,
                    toughest_opponent_code: toughest_code.unwrap_or_default(),
                    toughest_opponent_elo: toughest_elo.unwrap_or(0.0),
                },
            )
            .collect())
    }

    async fn matchups(&self, order: MatchupOrder, limit: i64) -> StoreResult<Vec<Matchup>> {
        let sql = match order {
            MatchupOrder::Strongest => MATCHUPS_DESC,
            MatchupOrder::Weakest => MATCHUPS_ASC,
        };
        let rows: Vec<MatchupRow> = sqlx::query_as(sql)
            .bind(limit)
            .fetch_all(&self.pool)
            .await?;
        Ok(rows.into_iter().map(matchup_from_row).collect())
    }

    async fn weakest_matchup_per_season(&self) -> StoreResult<Vec<Matchup>> {
        // A game belongs to the league of its teams; the ABA codes are bound as an array.
        let aba: Vec<String> = reference::ABA_TEAM_CODES
            .iter()
            .map(|c| c.to_string())
            .collect();
        let rows: Vec<MatchupRow> = sqlx::query_as(
            "SELECT DISTINCT ON (g.season, h.code = ANY($1))
                    g.season, g.game_date, h.code, h.name, a.code, a.name,
                    g.home_elo_pre, g.away_elo_pre, g.home_score, g.away_score, g.game_type = 'playoff' AS playoff
             FROM games g JOIN teams h ON h.id = g.home_team_id JOIN teams a ON a.id = g.away_team_id
             WHERE g.home_score IS NOT NULL AND (h.code = ANY($1)) = (a.code = ANY($1))
             ORDER BY g.season, h.code = ANY($1), g.home_elo_pre + g.away_elo_pre ASC, g.game_date",
        )
        .bind(&aba)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(matchup_from_row).collect())
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
