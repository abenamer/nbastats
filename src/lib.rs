//! NBA Elo Championship Simulator.
//!
//! Module map (domain logic is independent of infrastructure):
//! * `elo`, `simulation`, `playoffs`, `backtesting` – pure engines and mathematics.
//! * `models`, `reference` – domain types and static reference data.
//! * `db`, `imports`, `jobs`, `services`, `app` – PostgreSQL access and orchestration.
//! * `web` – Axum router, JSON API and server-rendered pages.
pub mod app;
pub mod backtesting;
pub mod config;
pub mod db;
pub mod elo;
pub mod imports;
pub mod jobs;
pub mod models;
pub mod playoffs;
pub mod reference;
pub mod services;
pub mod simulation;
pub mod web;
