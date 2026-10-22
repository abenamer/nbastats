//! `GameLog` and `GameStore` for PostgreSQL.

use chrono::NaiveDate;

use crate::application::ports::{
    EloPoint, GameLog, GameStore, NewChampionship, NewGame, SeasonSummary, StoreResult,
    UpsertCounts,
};
use crate::domain::ids::{FranchiseId, SeasonYear, TeamId};
use crate::domain::models::{EloSource, Franchise, Game, GameType, Season, Team, TeamSeason};
use crate::domain::reference;

use super::PostgresStore;

const GAME_COLS: &str = "id, season, game_date, seq, home_team_id, away_team_id, neutral, game_type, home_score, away_score,
    home_elo_pre, away_elo_pre, home_elo_post, away_elo_post, home_win_prob, elo_source";
const SEASON_COLS: &str = "season, label, team_count, game_count, playoff_game_count, play_in_game_count, cup_final_game_count,
    computed_game_count, scheduled_game_count, regular_season_end, playoffs_start, playoffs_end, complete";

impl GameLog for PostgresStore {
    async fn teams(&self) -> StoreResult<Vec<Team>> {
        Ok(sqlx::query_as::<_, Team>("SELECT id, code, franchise_id, name, first_season, last_season FROM teams ORDER BY code")
            .fetch_all(&self.pool)
            .await?)
    }

    async fn franchises(&self, active_only: bool) -> StoreResult<Vec<Franchise>> {
        Ok(sqlx::query_as::<_, Franchise>(
            "SELECT id, key, name, active FROM franchises WHERE active OR NOT $1 ORDER BY key",
        )
        .bind(active_only)
        .fetch_all(&self.pool)
        .await?)
    }

    async fn franchise_by_key(&self, key: &str) -> StoreResult<Option<Franchise>> {
        Ok(sqlx::query_as::<_, Franchise>(
            "SELECT id, key, name, active FROM franchises WHERE key = $1",
        )
        .bind(key)
        .fetch_optional(&self.pool)
        .await?)
    }

    async fn seasons(&self) -> StoreResult<Vec<Season>> {
        Ok(sqlx::query_as::<_, Season>(&format!(
            "SELECT {SEASON_COLS} FROM seasons ORDER BY season"
        ))
        .fetch_all(&self.pool)
        .await?)
    }

    async fn latest_season(&self) -> StoreResult<Option<SeasonYear>> {
        let r: (Option<SeasonYear>,) = sqlx::query_as("SELECT max(season) FROM seasons")
            .fetch_one(&self.pool)
            .await?;
        Ok(r.0)
    }

    async fn game_count(&self) -> StoreResult<i64> {
        let r: (i64,) = sqlx::query_as("SELECT count(*) FROM games")
            .fetch_one(&self.pool)
            .await?;
        Ok(r.0)
    }

    async fn season_games(&self, season: SeasonYear) -> StoreResult<Vec<Game>> {
        Ok(sqlx::query_as::<_, Game>(&format!(
            "SELECT {GAME_COLS} FROM games WHERE season = $1 ORDER BY game_date, seq, id"
        ))
        .bind(season)
        .fetch_all(&self.pool)
        .await?)
    }

    async fn season_games_until(
        &self,
        season: SeasonYear,
        cutoff: NaiveDate,
    ) -> StoreResult<Vec<Game>> {
        Ok(sqlx::query_as::<_, Game>(&format!(
            "SELECT {GAME_COLS} FROM games WHERE season = $1 AND game_date <= $2 ORDER BY game_date, seq, id"
        ))
        .bind(season)
        .bind(cutoff)
        .fetch_all(&self.pool)
        .await?)
    }

    async fn franchise_elo_history(
        &self,
        franchise: FranchiseId,
        from: SeasonYear,
        to: SeasonYear,
    ) -> StoreResult<Vec<EloPoint>> {
        let rows: Vec<(SeasonYear, NaiveDate, String, GameType, f64)> = sqlx::query_as(
            "SELECT g.season, g.game_date, t.code, g.game_type,
                    CASE WHEN g.home_team_id = t.id THEN g.home_elo_post ELSE g.away_elo_post END AS elo_post
             FROM games g JOIN teams t ON t.id IN (g.home_team_id, g.away_team_id)
             WHERE t.franchise_id = $1 AND g.season BETWEEN $2 AND $3
             ORDER BY g.game_date, g.seq, g.id",
        )
        .bind(franchise)
        .bind(from)
        .bind(to)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|(season, game_date, code, game_type, elo_post)| EloPoint {
                season,
                game_date,
                code,
                game_type,
                elo_post,
            })
            .collect())
    }
}

impl GameStore for PostgresStore {
    async fn sync_reference(&self) -> StoreResult<()> {
        let mut tx = self.pool.begin().await?;
        let mut seen = std::collections::HashSet::new();
        for t in reference::TEAM_LINEAGE {
            if !seen.insert(t.1) {
                continue;
            }
            let active = reference::TEAM_LINEAGE
                .iter()
                .any(|u| u.1 == t.1 && u.5 == reference::CURRENT);
            sqlx::query(
                "INSERT INTO franchises (key, name, active) VALUES ($1, $2, $3)
                 ON CONFLICT (key) DO UPDATE SET name = EXCLUDED.name, active = EXCLUDED.active",
            )
            .bind(t.1)
            .bind(t.2)
            .bind(active)
            .execute(&mut *tx)
            .await?;
        }
        for t in reference::TEAM_LINEAGE {
            let last: Option<i32> = (t.5 != reference::CURRENT).then_some(t.5);
            sqlx::query(
                "INSERT INTO teams (code, franchise_id, name, first_season, last_season)
                 VALUES ($1, (SELECT id FROM franchises WHERE key = $2), $3, $4, $5)
                 ON CONFLICT (code) DO UPDATE SET franchise_id = EXCLUDED.franchise_id, name = EXCLUDED.name,
                   first_season = EXCLUDED.first_season, last_season = EXCLUDED.last_season",
            )
            .bind(t.0)
            .bind(t.1)
            .bind(t.3)
            .bind(t.4)
            .bind(last)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    async fn sync_alignments(&self) -> StoreResult<()> {
        let teams = self.team_map().await?;
        let rows: Vec<(SeasonYear, TeamId)> = sqlx::query_as(
            "SELECT DISTINCT season, team_id FROM (
                SELECT season, home_team_id AS team_id FROM games
                UNION SELECT season, away_team_id FROM games) s WHERE season >= $1",
        )
        .bind(SeasonYear::FIRST_MODERN)
        .fetch_all(&self.pool)
        .await?;
        let mut tx = self.pool.begin().await?;
        for (season, team_id) in rows {
            let Some(team) = teams.get(&team_id) else {
                continue;
            };
            if let Some((conf, div)) = reference::alignment(season, &team.code) {
                sqlx::query(
                    "INSERT INTO alignments (season, team_id, conference, division) VALUES ($1, $2, $3, $4)
                     ON CONFLICT (season, team_id) DO UPDATE SET conference = EXCLUDED.conference, division = EXCLUDED.division",
                )
                .bind(season)
                .bind(team_id)
                .bind(conf.as_str())
                .bind(div)
                .execute(&mut *tx)
                .await?;
            }
        }
        tx.commit().await?;
        Ok(())
    }

    async fn ensure_season(&self, season: SeasonYear) -> StoreResult<()> {
        sqlx::query(
            "INSERT INTO seasons (season, label) VALUES ($1, $2) ON CONFLICT (season) DO NOTHING",
        )
        .bind(season)
        .bind(season.label())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn upsert_games(&self, import_id: i64, rows: &[NewGame]) -> StoreResult<UpsertCounts> {
        let mut counts = UpsertCounts::default();
        let mut tx = self.pool.begin().await?;
        for chunk in rows.chunks(2000) {
            let n = chunk.len();
            let season: Vec<SeasonYear> = chunk.iter().map(|r| r.season).collect();
            let date: Vec<NaiveDate> = chunk.iter().map(|r| r.date).collect();
            let seq: Vec<i16> = chunk.iter().map(|r| r.seq).collect();
            let home: Vec<TeamId> = chunk.iter().map(|r| r.home).collect();
            let away: Vec<TeamId> = chunk.iter().map(|r| r.away).collect();
            let neutral: Vec<bool> = chunk.iter().map(|r| r.neutral).collect();
            let gtype: Vec<GameType> = chunk.iter().map(|r| r.game_type).collect();
            let hs: Vec<Option<i32>> = chunk.iter().map(|r| r.home_score).collect();
            let as_: Vec<Option<i32>> = chunk.iter().map(|r| r.away_score).collect();
            let hpre: Vec<f64> = chunk.iter().map(|r| r.home_elo_pre).collect();
            let apre: Vec<f64> = chunk.iter().map(|r| r.away_elo_pre).collect();
            let hpost: Vec<f64> = chunk.iter().map(|r| r.home_elo_post).collect();
            let apost: Vec<f64> = chunk.iter().map(|r| r.away_elo_post).collect();
            let prob: Vec<Option<f64>> = chunk.iter().map(|r| r.home_win_prob).collect();
            let src: Vec<EloSource> = chunk.iter().map(|r| r.elo_source).collect();
            let returned: Vec<(bool,)> = sqlx::query_as(
                "INSERT INTO games (season, game_date, seq, home_team_id, away_team_id, neutral, game_type, home_score, away_score,
                    home_elo_pre, away_elo_pre, home_elo_post, away_elo_post, home_win_prob, import_id, elo_source)
                 SELECT s, d, q, h, a, n, t, hs, as_, hpre, apre, hpost, apost, p, $15, src
                 FROM UNNEST($1::int4[], $2::date[], $3::int2[], $4::int4[], $5::int4[], $6::bool[], $7::text[], $8::int4[], $9::int4[],
                             $10::float8[], $11::float8[], $12::float8[], $13::float8[], $14::float8[], $16::text[])
                      AS u(s, d, q, h, a, n, t, hs, as_, hpre, apre, hpost, apost, p, src)
                 ON CONFLICT (game_date, home_team_id, away_team_id, seq) DO UPDATE SET
                    season = EXCLUDED.season, neutral = EXCLUDED.neutral,
                    game_type = CASE WHEN games.game_type IN ('play_in', 'cup_final') AND EXCLUDED.game_type = 'regular'
                                     THEN games.game_type ELSE EXCLUDED.game_type END,
                    home_score = EXCLUDED.home_score, away_score = EXCLUDED.away_score,
                    home_elo_pre = EXCLUDED.home_elo_pre, away_elo_pre = EXCLUDED.away_elo_pre,
                    home_elo_post = EXCLUDED.home_elo_post, away_elo_post = EXCLUDED.away_elo_post,
                    home_win_prob = EXCLUDED.home_win_prob, import_id = EXCLUDED.import_id, elo_source = EXCLUDED.elo_source,
                    updated_at = now()
                 WHERE NOT (games.elo_source = 'imported' AND EXCLUDED.elo_source = 'computed')
                   AND (games.season, games.neutral, games.home_score, games.away_score, games.home_elo_pre, games.away_elo_pre,
                        games.home_elo_post, games.away_elo_post, games.home_win_prob, games.elo_source,
                        CASE WHEN games.game_type IN ('play_in', 'cup_final') THEN 'regular' ELSE games.game_type END)
                       IS DISTINCT FROM
                       (EXCLUDED.season, EXCLUDED.neutral, EXCLUDED.home_score, EXCLUDED.away_score, EXCLUDED.home_elo_pre,
                        EXCLUDED.away_elo_pre, EXCLUDED.home_elo_post, EXCLUDED.away_elo_post, EXCLUDED.home_win_prob,
                        EXCLUDED.elo_source, EXCLUDED.game_type)
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
            .bind(&src)
            .fetch_all(&mut *tx)
            .await?;
            let inserted = returned.iter().filter(|r| r.0).count() as i64;
            counts.inserted += inserted;
            counts.updated += returned.len() as i64 - inserted;
            counts.unchanged += n as i64 - returned.len() as i64;
        }
        tx.commit().await?;
        Ok(counts)
    }

    async fn delete_computed_games(&self, season: SeasonYear) -> StoreResult<u64> {
        let r = sqlx::query("DELETE FROM games WHERE season = $1 AND elo_source = $2")
            .bind(season)
            .bind(EloSource::Computed)
            .execute(&self.pool)
            .await?;
        Ok(r.rows_affected())
    }

    async fn delete_season(&self, season: SeasonYear) -> StoreResult<u64> {
        let mut tx = self.pool.begin().await?;
        for sql in [
            "DELETE FROM forecasts WHERE season = $1",
            "DELETE FROM championships WHERE season = $1",
            "DELETE FROM team_seasons WHERE season = $1",
        ] {
            sqlx::query(sql).bind(season).execute(&mut *tx).await?;
        }
        let games = sqlx::query("DELETE FROM games WHERE season = $1")
            .bind(season)
            .execute(&mut *tx)
            .await?
            .rows_affected();
        sqlx::query("DELETE FROM seasons WHERE season = $1")
            .bind(season)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(games)
    }

    async fn set_game_types(&self, ids: &[i64], game_type: GameType) -> StoreResult<()> {
        if ids.is_empty() {
            return Ok(());
        }
        sqlx::query("UPDATE games SET game_type = $2 WHERE id = ANY($1)")
            .bind(ids)
            .bind(game_type)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn update_season_summary(&self, s: &SeasonSummary) -> StoreResult<()> {
        sqlx::query(
            "UPDATE seasons SET team_count = $2, game_count = $3, playoff_game_count = $4, play_in_game_count = $5,
                cup_final_game_count = $6, regular_season_end = $7, playoffs_start = $8, playoffs_end = $9, complete = $10,
                computed_game_count = $11, scheduled_game_count = $12, updated_at = now() WHERE season = $1",
        )
        .bind(s.season)
        .bind(s.team_count)
        .bind(s.game_count)
        .bind(s.playoff_game_count)
        .bind(s.play_in_game_count)
        .bind(s.cup_final_game_count)
        .bind(s.regular_season_end)
        .bind(s.playoffs_start)
        .bind(s.playoffs_end)
        .bind(s.complete)
        .bind(s.computed_game_count)
        .bind(s.scheduled_game_count)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn upsert_team_season(&self, ts: &TeamSeason) -> StoreResult<()> {
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
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn upsert_championship(&self, c: &NewChampionship) -> StoreResult<()> {
        sqlx::query(
            "INSERT INTO championships (season, champion_team_id, champion_franchise_id, runner_up_team_id, source)
             VALUES ($1,$2,$3,$4,$5)
             ON CONFLICT (season) DO UPDATE SET champion_team_id = EXCLUDED.champion_team_id,
                champion_franchise_id = EXCLUDED.champion_franchise_id, runner_up_team_id = EXCLUDED.runner_up_team_id,
                source = EXCLUDED.source, updated_at = now()",
        )
        .bind(c.season)
        .bind(c.champion)
        .bind(c.champion_franchise)
        .bind(c.runner_up)
        .bind(c.source.as_str())
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}
