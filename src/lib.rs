//! NBA Elo Championship Simulator.
//!
//! Layout (REAS ports and adapters):
//! * [`domain`] – pure types, mathematics and engines.
//! * [`application`] – use cases, ports and shared state.
//! * [`adapters`] – PostgreSQL, HTTP, CSV and dataset download implementations.
//! * [`config`] – environment configuration; the composition root is `main.rs`.
pub mod adapters;
pub mod application;
pub mod config;
pub mod domain;
