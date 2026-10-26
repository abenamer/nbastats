//! `PlayerStore` on PostgreSQL: players by source identifier, box-score lines matched to
//! stored games, the dates fetched, and the injury reports.

use std::collections::HashMap;

use chrono::{DateTime, NaiveDate, Utc};
use sqlx::postgres::PgRow;
use sqlx::{FromRow, PgConnection, Row};

use crate::application::ports::{
    Appearance, BoxScoreCounts, BoxScoreFetch, BoxScoreLine, InjuryLine, InjuryRecord, Player,
    PlayerCoverage, PlayerRecord, PlayerSeason, PlayerStore, SeasonBackfill, StoreResult,
};
use crate::domain::ids::{PlayerId, SeasonYear, TeamId};
use crate::domain::worth::Line;

use super::PostgresStore;

/// The store's team identifiers by code.
async fn team_ids_by_code(conn: &mut PgConnection) -> StoreResult<HashMap<String, TeamId>> {
    let rows: Vec<(String, TeamId)> = sqlx::query_as("SELECT code, id FROM teams")
        .fetch_all(&mut *conn)
        .await?;
    Ok(rows.into_iter().collect())
}

/// Upsert the players and return their ids by source identifier. A blank field never
/// overwrites a filled one; a current team the store does not know leaves the stored one.
async fn upsert_players_in(
    conn: &mut PgConnection,
    source: &str,
    players: &[&PlayerRecord],
) -> StoreResult<HashMap<i64, PlayerId>> {
    let teams = team_ids_by_code(conn).await?;
    let mut ids = HashMap::with_capacity(players.len());
    for p in players {
        let team_id = p.team_code.as_deref().and_then(|c| teams.get(c)).copied();
        let (id,): (PlayerId,) = sqlx::query_as(
            "INSERT INTO players (source, source_id, first_name, last_name, position, height, weight,
                                  college, country, draft_year, draft_round, draft_number,
                                  jersey_number, team_id, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, now())
             ON CONFLICT (source, source_id) DO UPDATE SET
                first_name = CASE WHEN EXCLUDED.first_name = '' THEN players.first_name ELSE EXCLUDED.first_name END,
                last_name = CASE WHEN EXCLUDED.last_name = '' THEN players.last_name ELSE EXCLUDED.last_name END,
                position = COALESCE(EXCLUDED.position, players.position),
                height = COALESCE(EXCLUDED.height, players.height),
                weight = COALESCE(EXCLUDED.weight, players.weight),
                college = COALESCE(EXCLUDED.college, players.college),
                country = COALESCE(EXCLUDED.country, players.country),
                draft_year = COALESCE(EXCLUDED.draft_year, players.draft_year),
                draft_round = COALESCE(EXCLUDED.draft_round, players.draft_round),
                draft_number = COALESCE(EXCLUDED.draft_number, players.draft_number),
                jersey_number = COALESCE(EXCLUDED.jersey_number, players.jersey_number),
                team_id = COALESCE(EXCLUDED.team_id, players.team_id),
                updated_at = now()
             RETURNING id",
        )
        .bind(source)
        .bind(p.source_id)
        .bind(&p.first_name)
        .bind(&p.last_name)
        .bind(&p.position)
        .bind(&p.height)
        .bind(&p.weight)
        .bind(&p.college)
        .bind(&p.country)
        .bind(p.draft_year)
        .bind(p.draft_round)
        .bind(p.draft_number)
        .bind(&p.jersey_number)
        .bind(team_id)
        .fetch_one(&mut *conn)
        .await?;
        ids.insert(p.source_id, id);
    }
    Ok(ids)
}

/// The distinct players of a set of records, first occurrence kept.
fn distinct_players<'a>(records: impl Iterator<Item = &'a PlayerRecord>) -> Vec<&'a PlayerRecord> {
    let mut seen = std::collections::HashSet::new();
    records.filter(|p| seen.insert(p.source_id)).collect()
}

/// The stored games of `date`: (team code, game id, team id, home) for both sides.
async fn games_on(
    conn: &mut PgConnection,
    date: NaiveDate,
) -> StoreResult<HashMap<String, (i64, TeamId, bool)>> {
    let rows: Vec<(i64, String, TeamId, String, TeamId)> = sqlx::query_as(
        "SELECT g.id, h.code, h.id, a.code, a.id
         FROM games g JOIN teams h ON h.id = g.home_team_id JOIN teams a ON a.id = g.away_team_id
         WHERE g.game_date = $1",
    )
    .bind(date)
    .fetch_all(&mut *conn)
    .await?;
    let mut out = HashMap::with_capacity(rows.len() * 2);
    for (game_id, home_code, home_id, away_code, away_id) in rows {
        out.insert(home_code, (game_id, home_id, true));
        out.insert(away_code, (game_id, away_id, false));
    }
    Ok(out)
}

/// One appearance row. A struct, not a tuple: the row has more columns than sqlx maps
/// through tuples.
struct AppearanceRow(Appearance);

impl FromRow<'_, PgRow> for AppearanceRow {
    fn from_row(r: &PgRow) -> Result<Self, sqlx::Error> {
        Ok(AppearanceRow(Appearance {
            game_id: r.try_get("game_id")?,
            season: r.try_get("season")?,
            game_date: r.try_get("game_date")?,
            game_type: r.try_get("game_type")?,
            player_id: r.try_get("player_id")?,
            team_id: r.try_get("team_id")?,
            team_code: r.try_get("team_code")?,
            opponent_code: r.try_get("opponent_code")?,
            home: r.try_get("home")?,
            seconds: r.try_get("seconds")?,
            pts: r.try_get("pts")?,
            reb: r.try_get("reb")?,
            oreb: r.try_get("oreb")?,
            dreb: r.try_get("dreb")?,
            ast: r.try_get("ast")?,
            stl: r.try_get("stl")?,
            blk: r.try_get("blk")?,
            tov: r.try_get("tov")?,
            pf: r.try_get("pf")?,
            fgm: r.try_get("fgm")?,
            fga: r.try_get("fga")?,
            fg3m: r.try_get("fg3m")?,
            fg3a: r.try_get("fg3a")?,
            ftm: r.try_get("ftm")?,
            fta: r.try_get("fta")?,
            plus_minus: r.try_get("plus_minus")?,
        }))
    }
}

const APPEARANCE_SELECT: &str = "SELECT a.game_id, g.season, g.game_date, g.game_type, a.player_id, a.team_id, t.code AS team_code,
            CASE WHEN g.home_team_id = a.team_id THEN o.code ELSE h.code END AS opponent_code,
            g.home_team_id = a.team_id AS home,
            a.seconds, a.pts, a.reb, a.oreb, a.dreb, a.ast, a.stl, a.blk, a.tov, a.pf,
            a.fgm, a.fga, a.fg3m, a.fg3a, a.ftm, a.fta, a.plus_minus
     FROM appearances a
     JOIN games g ON g.id = a.game_id
     JOIN teams t ON t.id = a.team_id
     JOIN teams h ON h.id = g.home_team_id
     JOIN teams o ON o.id = g.away_team_id";

/// One player row, with the current team's code joined in.
struct PlayerRow(Player);

impl FromRow<'_, PgRow> for PlayerRow {
    fn from_row(r: &PgRow) -> Result<Self, sqlx::Error> {
        Ok(PlayerRow(Player {
            id: r.try_get("id")?,
            source: r.try_get("source")?,
            source_id: r.try_get("source_id")?,
            first_name: r.try_get("first_name")?,
            last_name: r.try_get("last_name")?,
            position: r.try_get("position")?,
            height: r.try_get("height")?,
            weight: r.try_get("weight")?,
            college: r.try_get("college")?,
            country: r.try_get("country")?,
            draft_year: r.try_get("draft_year")?,
            draft_round: r.try_get("draft_round")?,
            draft_number: r.try_get("draft_number")?,
            jersey_number: r.try_get("jersey_number")?,
            team_code: r.try_get("team_code")?,
            active: r.try_get("active")?,
        }))
    }
}

const PLAYER_SELECT: &str =
    "SELECT p.id, p.source, p.source_id, p.first_name, p.last_name, p.position, p.height, p.weight,
            p.college, p.country, p.draft_year, p.draft_round, p.draft_number, p.jersey_number,
            t.code AS team_code, p.active
     FROM players p LEFT JOIN teams t ON t.id = p.team_id";

fn player_from_row(r: PlayerRow) -> Player {
    r.0
}

impl PlayerStore for PostgresStore {
    async fn save_box_scores(
        &self,
        date: NaiveDate,
        source: &str,
        lines: &[BoxScoreLine],
    ) -> StoreResult<BoxScoreCounts> {
        let mut tx = self.pool.begin().await?;
        // An empty answer for a date that has lines keeps them: a fetch of the settling
        // window that comes back empty (a gap at the source) must not erase a stored day.
        if lines.is_empty() {
            let (stored,): (i64,) = sqlx::query_as(
                "SELECT count(*) FROM appearances a JOIN games g ON g.id = a.game_id WHERE g.game_date = $1",
            )
            .bind(date)
            .fetch_one(&mut *tx)
            .await?;
            if stored > 0 {
                return Ok(BoxScoreCounts::default());
            }
        }
        let games = games_on(&mut tx, date).await?;
        let matched: Vec<&BoxScoreLine> = lines
            .iter()
            .filter(|l| l.date == date && games.contains_key(&l.team_code))
            .collect();
        let players = distinct_players(matched.iter().map(|l| &l.player));
        let ids = upsert_players_in(&mut tx, source, &players).await?;
        // Replace the date's lines, so a second fetch of the same date leaves no stale line.
        sqlx::query(
            "DELETE FROM appearances a USING games g WHERE g.id = a.game_id AND g.game_date = $1",
        )
        .bind(date)
        .execute(&mut *tx)
        .await?;
        for l in &matched {
            let (game_id, team_id, _) = games[&l.team_code];
            let player_id = ids[&l.player.source_id];
            sqlx::query(
                "INSERT INTO appearances (game_id, player_id, team_id, seconds, pts, reb, oreb, dreb, ast,
                                          stl, blk, tov, pf, fgm, fga, fg3m, fg3a, ftm, fta, plus_minus)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19, $20)
                 ON CONFLICT (game_id, player_id) DO UPDATE SET team_id = EXCLUDED.team_id,
                    seconds = EXCLUDED.seconds, pts = EXCLUDED.pts, reb = EXCLUDED.reb, oreb = EXCLUDED.oreb,
                    dreb = EXCLUDED.dreb, ast = EXCLUDED.ast, stl = EXCLUDED.stl, blk = EXCLUDED.blk,
                    tov = EXCLUDED.tov, pf = EXCLUDED.pf, fgm = EXCLUDED.fgm, fga = EXCLUDED.fga,
                    fg3m = EXCLUDED.fg3m, fg3a = EXCLUDED.fg3a, ftm = EXCLUDED.ftm, fta = EXCLUDED.fta,
                    plus_minus = EXCLUDED.plus_minus",
            )
            .bind(game_id)
            .bind(player_id)
            .bind(team_id)
            .bind(l.seconds)
            .bind(l.pts)
            .bind(l.reb)
            .bind(l.oreb)
            .bind(l.dreb)
            .bind(l.ast)
            .bind(l.stl)
            .bind(l.blk)
            .bind(l.tov)
            .bind(l.pf)
            .bind(l.fgm)
            .bind(l.fga)
            .bind(l.fg3m)
            .bind(l.fg3a)
            .bind(l.ftm)
            .bind(l.fta)
            .bind(l.plus_minus)
            .execute(&mut *tx)
            .await?;
        }
        let counts = BoxScoreCounts {
            lines: lines.len() as i64,
            matched: matched.len() as i64,
            unmatched: (lines.len() - matched.len()) as i64,
            players_upserted: players.len() as i64,
        };
        sqlx::query(
            "INSERT INTO box_score_fetches (game_date, source, fetched_at, lines, matched, unmatched)
             VALUES ($1, $2, now(), $3, $4, $5)
             ON CONFLICT (game_date) DO UPDATE SET source = EXCLUDED.source, fetched_at = now(),
                lines = EXCLUDED.lines, matched = EXCLUDED.matched, unmatched = EXCLUDED.unmatched",
        )
        .bind(date)
        .bind(source)
        .bind(counts.lines as i32)
        .bind(counts.matched as i32)
        .bind(counts.unmatched as i32)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(counts)
    }

    async fn dates_without_box_scores(&self, season: SeasonYear) -> StoreResult<Vec<NaiveDate>> {
        let rows: Vec<(NaiveDate,)> = sqlx::query_as(
            "SELECT DISTINCT g.game_date FROM games g
             LEFT JOIN box_score_fetches f ON f.game_date = g.game_date
             WHERE g.season = $1 AND g.home_score IS NOT NULL AND f.game_date IS NULL
             ORDER BY g.game_date",
        )
        .bind(season)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(|r| r.0).collect())
    }

    async fn box_score_fetches(&self, season: SeasonYear) -> StoreResult<Vec<BoxScoreFetch>> {
        let rows: Vec<(NaiveDate, String, DateTime<Utc>, i32, i32, i32)> = sqlx::query_as(
            "SELECT DISTINCT f.game_date, f.source, f.fetched_at, f.lines, f.matched, f.unmatched
             FROM box_score_fetches f JOIN games g ON g.game_date = f.game_date
             WHERE g.season = $1 ORDER BY f.game_date",
        )
        .bind(season)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(
                |(game_date, source, fetched_at, lines, matched, unmatched)| BoxScoreFetch {
                    game_date,
                    source,
                    fetched_at,
                    counts: BoxScoreCounts {
                        lines: i64::from(lines),
                        matched: i64::from(matched),
                        unmatched: i64::from(unmatched),
                        players_upserted: 0,
                    },
                },
            )
            .collect())
    }

    async fn save_injury_report(
        &self,
        date: NaiveDate,
        source: &str,
        lines: &[InjuryRecord],
    ) -> StoreResult<i64> {
        let mut tx = self.pool.begin().await?;
        let teams: HashMap<String, TeamId> =
            sqlx::query_as::<_, (String, TeamId)>("SELECT code, id FROM teams")
                .fetch_all(&mut *tx)
                .await?
                .into_iter()
                .collect();
        let known: Vec<&InjuryRecord> = lines
            .iter()
            .filter(|l| teams.contains_key(&l.team_code))
            .collect();
        let players = distinct_players(known.iter().map(|l| &l.player));
        let ids = upsert_players_in(&mut tx, source, &players).await?;
        sqlx::query("DELETE FROM injury_reports WHERE report_date = $1")
            .bind(date)
            .execute(&mut *tx)
            .await?;
        let mut stored = 0i64;
        for l in &known {
            let done = sqlx::query(
                "INSERT INTO injury_reports (report_date, player_id, team_id, status, return_date, description, fetched_at)
                 VALUES ($1, $2, $3, $4, $5, $6, now())
                 ON CONFLICT (report_date, player_id) DO UPDATE SET team_id = EXCLUDED.team_id,
                    status = EXCLUDED.status, return_date = EXCLUDED.return_date,
                    description = EXCLUDED.description, fetched_at = now()",
            )
            .bind(date)
            .bind(ids[&l.player.source_id])
            .bind(teams[&l.team_code])
            .bind(&l.status)
            .bind(l.return_date)
            .bind(&l.description)
            .execute(&mut *tx)
            .await?;
            stored += done.rows_affected() as i64;
        }
        tx.commit().await?;
        Ok(stored)
    }

    async fn injury_report(&self, date: NaiveDate) -> StoreResult<Vec<InjuryLine>> {
        type Row = (
            NaiveDate,
            PlayerId,
            String,
            String,
            String,
            String,
            Option<NaiveDate>,
            Option<String>,
        );
        let rows: Vec<Row> = sqlx::query_as(
            "SELECT r.report_date, r.player_id, p.first_name, p.last_name, t.code, r.status, r.return_date, r.description
             FROM injury_reports r JOIN players p ON p.id = r.player_id JOIN teams t ON t.id = r.team_id
             WHERE r.report_date = $1 ORDER BY t.code, p.last_name, p.first_name",
        )
        .bind(date)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(
                |(
                    report_date,
                    player_id,
                    first,
                    last,
                    team_code,
                    status,
                    return_date,
                    description,
                )| {
                    InjuryLine {
                        report_date,
                        player_id,
                        player_name: format!("{first} {last}"),
                        team_code,
                        status,
                        return_date,
                        description,
                    }
                },
            )
            .collect())
    }

    async fn latest_injury_report_date(&self) -> StoreResult<Option<NaiveDate>> {
        let (date,): (Option<NaiveDate>,) =
            sqlx::query_as("SELECT max(report_date) FROM injury_reports")
                .fetch_one(&self.pool)
                .await?;
        Ok(date)
    }

    async fn save_players(&self, source: &str, players: &[PlayerRecord]) -> StoreResult<i64> {
        let mut tx = self.pool.begin().await?;
        let distinct = distinct_players(players.iter());
        upsert_players_in(&mut tx, source, &distinct).await?;
        tx.commit().await?;
        Ok(distinct.len() as i64)
    }

    async fn save_active_players(
        &self,
        source: &str,
        players: &[PlayerRecord],
    ) -> StoreResult<i64> {
        let mut tx = self.pool.begin().await?;
        let distinct = distinct_players(players.iter());
        let ids = upsert_players_in(&mut tx, source, &distinct).await?;
        let active: Vec<PlayerId> = ids.values().copied().collect();
        sqlx::query("UPDATE players SET active = (id = ANY($1)) WHERE source = $2")
            .bind(&active)
            .bind(source)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(active.len() as i64)
    }

    async fn search_players(&self, query: &str, limit: i64) -> StoreResult<Vec<Player>> {
        // The escape character is one backslash, so a backslash, a percent sign and an
        // underscore in the query match themselves.
        let escaped = query
            .trim()
            .replace('\\', "\\\\")
            .replace('%', "\\%")
            .replace('_', "\\_");
        let pattern = format!("%{escaped}%");
        let rows: Vec<PlayerRow> = sqlx::query_as(&format!(
            "{PLAYER_SELECT} WHERE (p.first_name || ' ' || p.last_name) ILIKE $1 ESCAPE '\\'
             ORDER BY p.last_name, p.first_name, p.id LIMIT $2"
        ))
        .bind(pattern)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(player_from_row).collect())
    }

    async fn player(&self, id: PlayerId) -> StoreResult<Option<Player>> {
        let row: Option<PlayerRow> = sqlx::query_as(&format!("{PLAYER_SELECT} WHERE p.id = $1"))
            .bind(id)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row.map(player_from_row))
    }

    async fn player_appearances(
        &self,
        id: PlayerId,
        season: SeasonYear,
    ) -> StoreResult<Vec<Appearance>> {
        let rows: Vec<AppearanceRow> = sqlx::query_as(&format!(
            "{APPEARANCE_SELECT} WHERE a.player_id = $1 AND g.season = $2 ORDER BY g.game_date, g.seq"
        ))
        .bind(id)
        .bind(season)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(|r| r.0).collect())
    }

    async fn player_seasons(&self, id: PlayerId) -> StoreResult<Vec<PlayerSeason>> {
        type Row = (SeasonYear, String, i64, i64, Option<i64>, Option<i64>);
        let rows: Vec<Row> = sqlx::query_as(
            "SELECT g.season, t.code, count(*),
                    count(*) FILTER (WHERE g.game_type = 'playoff'),
                    sum(a.seconds), sum(a.pts)
             FROM appearances a JOIN games g ON g.id = a.game_id JOIN teams t ON t.id = a.team_id
             WHERE a.player_id = $1
             GROUP BY g.season, t.code ORDER BY g.season, t.code",
        )
        .bind(id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(
                |(season, team_code, games, playoff_games, seconds, pts)| PlayerSeason {
                    season,
                    team_code,
                    games,
                    playoff_games,
                    seconds,
                    pts,
                },
            )
            .collect())
    }

    async fn game_appearances(&self, game_id: i64) -> StoreResult<Vec<Appearance>> {
        let rows: Vec<AppearanceRow> = sqlx::query_as(&format!(
            "{APPEARANCE_SELECT} WHERE a.game_id = $1
             ORDER BY (g.home_team_id = a.team_id) DESC, a.seconds DESC NULLS LAST, a.player_id"
        ))
        .bind(game_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(|r| r.0).collect())
    }

    async fn season_lines(&self, season: SeasonYear) -> StoreResult<Vec<Line>> {
        let rows: Vec<(i64, PlayerId, TeamId, bool)> = sqlx::query_as(
            "SELECT a.game_id, a.player_id, a.team_id,
                    COALESCE(a.seconds, 0) > 0 OR (a.seconds IS NULL AND
                        COALESCE(a.pts, 0) + COALESCE(a.fga, 0) + COALESCE(a.fta, 0)
                        + COALESCE(a.reb, 0) + COALESCE(a.ast, 0) > 0) AS played
             FROM appearances a JOIN games g ON g.id = a.game_id
             WHERE g.season = $1",
        )
        .bind(season)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|(game_id, player_id, team_id, played)| Line {
                game_id,
                player_id,
                team_id,
                played,
            })
            .collect())
    }

    async fn players_by_id(&self, ids: &[PlayerId]) -> StoreResult<Vec<Player>> {
        if ids.is_empty() {
            return Ok(vec![]);
        }
        let rows: Vec<PlayerRow> = sqlx::query_as(&format!("{PLAYER_SELECT} WHERE p.id = ANY($1)"))
            .bind(ids)
            .fetch_all(&self.pool)
            .await?;
        Ok(rows.into_iter().map(player_from_row).collect())
    }

    async fn backfill_progress(&self) -> StoreResult<Vec<SeasonBackfill>> {
        let rows: Vec<(SeasonYear, i64, i64)> = sqlx::query_as(
            "SELECT d.season, count(*), count(f.game_date)
             FROM (SELECT DISTINCT season, game_date FROM games WHERE home_score IS NOT NULL) d
             LEFT JOIN box_score_fetches f ON f.game_date = d.game_date
             GROUP BY d.season ORDER BY d.season",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|(season, dates_total, dates_fetched)| SeasonBackfill {
                season,
                dates_total,
                dates_fetched,
            })
            .collect())
    }

    async fn player_coverage(&self) -> StoreResult<PlayerCoverage> {
        type Row = (
            i64,
            i64,
            i64,
            i64,
            Option<NaiveDate>,
            Option<NaiveDate>,
            i64,
            Option<NaiveDate>,
        );
        let row: Row = sqlx::query_as(
            "SELECT (SELECT count(*) FROM players), (SELECT count(*) FROM players WHERE active),
                    (SELECT count(*) FROM appearances),
                    (SELECT count(*) FROM box_score_fetches),
                    (SELECT min(game_date) FROM box_score_fetches),
                    (SELECT max(game_date) FROM box_score_fetches),
                    (SELECT count(DISTINCT report_date) FROM injury_reports),
                    (SELECT max(report_date) FROM injury_reports)",
        )
        .fetch_one(&self.pool)
        .await?;
        Ok(PlayerCoverage {
            players: row.0,
            active_players: row.1,
            appearances: row.2,
            dates_fetched: row.3,
            first_date_fetched: row.4,
            last_date_fetched: row.5,
            injury_report_dates: row.6,
            last_injury_report: row.7,
        })
    }
}
