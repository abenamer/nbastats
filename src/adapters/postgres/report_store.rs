//! `ReportStore` on PostgreSQL: report links by date and team codes, and the dates looked up.

use chrono::{DateTime, NaiveDate, Utc};

use crate::application::ports::{GameReport, ReportLookup, ReportStore, StoreResult};

use super::PostgresStore;

impl ReportStore for PostgresStore {
    async fn reports_on(&self, dates: &[NaiveDate]) -> StoreResult<Vec<GameReport>> {
        type Row = (NaiveDate, String, String, String, Option<String>, String);
        let rows: Vec<Row> = sqlx::query_as(
            "SELECT game_date, home_code, away_code, url, title, source
             FROM game_reports WHERE game_date = ANY($1::date[])
             ORDER BY game_date, home_code, away_code",
        )
        .bind(dates)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(
                |(game_date, home_code, away_code, url, title, source)| GameReport {
                    game_date,
                    home_code,
                    away_code,
                    url,
                    title,
                    source,
                },
            )
            .collect())
    }

    async fn lookups_on(&self, dates: &[NaiveDate]) -> StoreResult<Vec<ReportLookup>> {
        let rows: Vec<(NaiveDate, String, DateTime<Utc>, i32)> = sqlx::query_as(
            "SELECT game_date, source, fetched_at, found FROM report_lookups
             WHERE game_date = ANY($1::date[])",
        )
        .bind(dates)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|(game_date, source, fetched_at, found)| ReportLookup {
                game_date,
                source,
                fetched_at,
                found,
            })
            .collect())
    }

    async fn save_lookup(
        &self,
        date: NaiveDate,
        source: &str,
        reports: &[GameReport],
    ) -> StoreResult<()> {
        let mut tx = self.pool.begin().await?;
        for r in reports {
            sqlx::query(
                "INSERT INTO game_reports (game_date, home_code, away_code, url, title, source)
                 VALUES ($1, $2, $3, $4, $5, $6)
                 ON CONFLICT (game_date, home_code, away_code) DO UPDATE SET
                    url = EXCLUDED.url, title = EXCLUDED.title, source = EXCLUDED.source, fetched_at = now()",
            )
            .bind(r.game_date)
            .bind(&r.home_code)
            .bind(&r.away_code)
            .bind(&r.url)
            .bind(&r.title)
            .bind(&r.source)
            .execute(&mut *tx)
            .await?;
        }
        sqlx::query(
            "INSERT INTO report_lookups (game_date, source, fetched_at, found) VALUES ($1, $2, now(), $3)
             ON CONFLICT (game_date) DO UPDATE SET source = EXCLUDED.source, fetched_at = now(), found = EXCLUDED.found",
        )
        .bind(date)
        .bind(source)
        .bind(reports.len() as i32)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(())
    }
}
