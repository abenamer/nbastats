//! `MarketStore` on PostgreSQL: the market snapshots, by date and team codes, and the title
//! futures, by day and team.

use sqlx::postgres::PgRow;
use sqlx::Row;

use crate::application::ports::{MarketStore, StoreResult};
use crate::domain::futures::FuturesSnapshot;
use crate::domain::ids::SeasonYear;
use crate::domain::tracking::Snapshot;

use super::PostgresStore;

const COLUMNS: &str = "game_date, home_code, away_code, season, captured_at, start_time,
    model_version, app_version, home_advantage, home_rating, away_rating, home_stored,
    away_stored, home_penalty, away_penalty, p_home_full, p_home_model, home_margin_model,
    provider, source, home_spread, home_spread_odds, away_spread_odds, home_moneyline,
    away_moneyline, total, p_home_market, book_margin";

fn snapshot(r: &PgRow) -> Result<Snapshot, sqlx::Error> {
    Ok(Snapshot {
        game_date: r.try_get("game_date")?,
        home_code: r.try_get("home_code")?,
        away_code: r.try_get("away_code")?,
        season: SeasonYear::of(r.try_get::<i32, _>("season")?),
        captured_at: r.try_get("captured_at")?,
        start_time: r.try_get("start_time")?,
        model_version: r.try_get("model_version")?,
        app_version: r.try_get("app_version")?,
        home_advantage: r.try_get("home_advantage")?,
        home_rating: r.try_get("home_rating")?,
        away_rating: r.try_get("away_rating")?,
        home_stored: r.try_get("home_stored")?,
        away_stored: r.try_get("away_stored")?,
        home_penalty: r.try_get("home_penalty")?,
        away_penalty: r.try_get("away_penalty")?,
        p_home_full: r.try_get("p_home_full")?,
        p_home_model: r.try_get("p_home_model")?,
        home_margin_model: r.try_get("home_margin_model")?,
        provider: r.try_get("provider")?,
        source: r.try_get("source")?,
        home_spread: r.try_get("home_spread")?,
        home_spread_odds: r.try_get("home_spread_odds")?,
        away_spread_odds: r.try_get("away_spread_odds")?,
        home_moneyline: r.try_get("home_moneyline")?,
        away_moneyline: r.try_get("away_moneyline")?,
        total: r.try_get("total")?,
        p_home_market: r.try_get("p_home_market")?,
        book_margin: r.try_get("book_margin")?,
    })
}

impl MarketStore for PostgresStore {
    async fn save_snapshots(&self, snapshots: &[Snapshot]) -> StoreResult<u64> {
        let mut tx = self.pool.begin().await?;
        let mut written = 0;
        for s in snapshots {
            // A stored capture gives way only to a later one taken before the start: the
            // new start when the capture has one (a moved tip), else the stored one. A
            // capture without a start keeps the stored start.
            let done = sqlx::query(&format!(
                "INSERT INTO market_snapshots ({COLUMNS}) VALUES ($1, $2, $3, $4, $5, $6, $7,
                 $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19, $20, $21, $22, $23,
                 $24, $25, $26, $27, $28)
                 ON CONFLICT (game_date, home_code, away_code) DO UPDATE SET
                    season = EXCLUDED.season, captured_at = EXCLUDED.captured_at,
                    start_time = COALESCE(EXCLUDED.start_time, market_snapshots.start_time),
                    model_version = EXCLUDED.model_version,
                    app_version = EXCLUDED.app_version, home_advantage = EXCLUDED.home_advantage,
                    home_rating = EXCLUDED.home_rating, away_rating = EXCLUDED.away_rating,
                    home_stored = EXCLUDED.home_stored, away_stored = EXCLUDED.away_stored,
                    home_penalty = EXCLUDED.home_penalty, away_penalty = EXCLUDED.away_penalty,
                    p_home_full = EXCLUDED.p_home_full, p_home_model = EXCLUDED.p_home_model,
                    home_margin_model = EXCLUDED.home_margin_model,
                    provider = EXCLUDED.provider, source = EXCLUDED.source,
                    home_spread = EXCLUDED.home_spread,
                    home_spread_odds = EXCLUDED.home_spread_odds,
                    away_spread_odds = EXCLUDED.away_spread_odds,
                    home_moneyline = EXCLUDED.home_moneyline,
                    away_moneyline = EXCLUDED.away_moneyline, total = EXCLUDED.total,
                    p_home_market = EXCLUDED.p_home_market, book_margin = EXCLUDED.book_margin
                 WHERE market_snapshots.captured_at < EXCLUDED.captured_at
                   AND EXCLUDED.captured_at < COALESCE(EXCLUDED.start_time,
                        market_snapshots.start_time, 'infinity'::timestamptz)"
            ))
            .bind(s.game_date)
            .bind(&s.home_code)
            .bind(&s.away_code)
            .bind(i32::from(s.season))
            .bind(s.captured_at)
            .bind(s.start_time)
            .bind(&s.model_version)
            .bind(&s.app_version)
            .bind(s.home_advantage)
            .bind(s.home_rating)
            .bind(s.away_rating)
            .bind(s.home_stored)
            .bind(s.away_stored)
            .bind(s.home_penalty)
            .bind(s.away_penalty)
            .bind(s.p_home_full)
            .bind(s.p_home_model)
            .bind(s.home_margin_model)
            .bind(&s.provider)
            .bind(&s.source)
            .bind(s.home_spread)
            .bind(s.home_spread_odds)
            .bind(s.away_spread_odds)
            .bind(s.home_moneyline)
            .bind(s.away_moneyline)
            .bind(s.total)
            .bind(s.p_home_market)
            .bind(s.book_margin)
            .execute(&mut *tx)
            .await?;
            written += done.rows_affected();
        }
        tx.commit().await?;
        Ok(written)
    }

    async fn snapshots(&self, season: SeasonYear) -> StoreResult<Vec<Snapshot>> {
        let rows = sqlx::query(&format!(
            "SELECT {COLUMNS} FROM market_snapshots WHERE season = $1
             ORDER BY game_date, home_code, away_code"
        ))
        .bind(i32::from(season))
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.iter().map(snapshot).collect::<Result<_, _>>()?)
    }

    async fn snapshot_seasons(&self) -> StoreResult<Vec<SeasonYear>> {
        let rows: Vec<(i32,)> =
            sqlx::query_as("SELECT DISTINCT season FROM market_snapshots ORDER BY season")
                .fetch_all(&self.pool)
                .await?;
        Ok(rows.into_iter().map(|(s,)| SeasonYear::of(s)).collect())
    }

    async fn save_futures(&self, snapshots: &[FuturesSnapshot]) -> StoreResult<u64> {
        let mut tx = self.pool.begin().await?;
        let mut written = 0;
        // A day's field is replaced whole: a team the new capture no longer lists goes, so
        // a later field of fewer teams leaves no stale row behind. Only rows older than the
        // day's latest capture in the batch go, as only they give way to it.
        let mut days: Vec<(i32, chrono::NaiveDate, chrono::DateTime<chrono::Utc>)> = Vec::new();
        for s in snapshots {
            let key = (i32::from(s.season), s.snapshot_date, s.captured_at);
            match days.iter_mut().find(|d| d.0 == key.0 && d.1 == key.1) {
                Some(d) => d.2 = d.2.max(key.2),
                None => days.push(key),
            }
        }
        for (season, day, captured_at) in &days {
            let codes: Vec<String> = snapshots
                .iter()
                .filter(|s| i32::from(s.season) == *season && s.snapshot_date == *day)
                .map(|s| s.team_code.clone())
                .collect();
            // Never reached from the rows above, and an empty field must not clear a day.
            if codes.is_empty() {
                continue;
            }
            sqlx::query(
                "DELETE FROM futures_snapshots WHERE season = $1 AND snapshot_date = $2
                 AND captured_at < $3 AND team_code <> ALL($4)",
            )
            .bind(season)
            .bind(day)
            .bind(captured_at)
            .bind(&codes)
            .execute(&mut *tx)
            .await?;
        }
        for s in snapshots {
            let done = sqlx::query(&format!(
                "INSERT INTO futures_snapshots ({FUTURES_COLUMNS})
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
                 ON CONFLICT (season, snapshot_date, team_code) DO UPDATE SET
                    captured_at = EXCLUDED.captured_at, provider = EXCLUDED.provider,
                    source = EXCLUDED.source, price = EXCLUDED.price,
                    p_market = EXCLUDED.p_market, book_margin = EXCLUDED.book_margin,
                    p_model = EXCLUDED.p_model, p_model_se = EXCLUDED.p_model_se,
                    model_version = EXCLUDED.model_version, app_version = EXCLUDED.app_version
                 WHERE futures_snapshots.captured_at < EXCLUDED.captured_at"
            ))
            .bind(i32::from(s.season))
            .bind(s.snapshot_date)
            .bind(&s.team_code)
            .bind(s.captured_at)
            .bind(&s.provider)
            .bind(&s.source)
            .bind(s.price)
            .bind(s.p_market)
            .bind(s.book_margin)
            .bind(s.p_model)
            .bind(s.p_model_se)
            .bind(&s.model_version)
            .bind(&s.app_version)
            .execute(&mut *tx)
            .await?;
            written += done.rows_affected();
        }
        tx.commit().await?;
        Ok(written)
    }

    async fn futures(&self, season: SeasonYear) -> StoreResult<Vec<FuturesSnapshot>> {
        let rows = sqlx::query(&format!(
            "SELECT {FUTURES_COLUMNS} FROM futures_snapshots WHERE season = $1
             ORDER BY snapshot_date, team_code"
        ))
        .bind(i32::from(season))
        .fetch_all(&self.pool)
        .await?;
        rows.iter()
            .map(|r| {
                Ok(FuturesSnapshot {
                    season: SeasonYear::of(r.try_get::<i32, _>("season")?),
                    snapshot_date: r.try_get("snapshot_date")?,
                    team_code: r.try_get("team_code")?,
                    captured_at: r.try_get("captured_at")?,
                    provider: r.try_get("provider")?,
                    source: r.try_get("source")?,
                    price: r.try_get("price")?,
                    p_market: r.try_get("p_market")?,
                    book_margin: r.try_get("book_margin")?,
                    p_model: r.try_get("p_model")?,
                    p_model_se: r.try_get("p_model_se")?,
                    model_version: r.try_get("model_version")?,
                    app_version: r.try_get("app_version")?,
                })
            })
            .collect::<Result<Vec<_>, sqlx::Error>>()
            .map_err(Into::into)
    }
}

const FUTURES_COLUMNS: &str = "season, snapshot_date, team_code, captured_at, provider, source,
    price, p_market, book_margin, p_model, p_model_se, model_version, app_version";
