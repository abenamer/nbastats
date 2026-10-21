//! Shared helpers for the database integration tests.
//! These tests need `DATABASE_URL` (see `.env`); `sqlx::test` creates a fresh database per test
//! and applies `./migrations`.

use std::collections::HashMap;

use nba_elo_simulator::adapters::postgres::{self as db, Pool};
use nba_elo_simulator::domain::models::Team;

pub const FIXTURE: &str = include_str!("../fixtures/mini_2016.csv");

pub async fn setup(pool: &Pool) -> HashMap<String, Team> {
    db::teams::sync_reference(pool).await.expect("reference sync");
    db::teams::team_code_map(pool).await.expect("teams")
}
