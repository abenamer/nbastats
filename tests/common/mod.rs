//! Shared helpers for the database integration tests.
//! These tests need `DATABASE_URL` (see `.env`); `sqlx::test` creates a fresh database per test
//! and applies `./migrations`.

use std::collections::HashMap;

use nba_elo_simulator::adapters::postgres::PostgresStore;
use nba_elo_simulator::application::ports::{GameLog, GameStore};
use nba_elo_simulator::domain::models::Team;
use sqlx::PgPool;

pub const FIXTURE: &str = include_str!("../fixtures/mini_2016.csv");

pub async fn setup(pool: PgPool) -> (PostgresStore, HashMap<String, Team>) {
    let store = PostgresStore::from_pool(pool);
    store.sync_reference().await.expect("reference sync");
    let teams = store.team_code_map().await.expect("teams");
    (store, teams)
}
