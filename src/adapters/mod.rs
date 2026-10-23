//! Adapters: concrete implementations at the system boundaries.
//! * `postgres` – outbound: the `PostgresStore` implements the application ports with SQLx.
//! * `http` – inbound: Axum router, JSON API and server-rendered pages.
//! * `csv` – inbound data format parsers for the Elo game logs.
//! * `dataset` – outbound: HTTP download of the source dataset.
//! * `balldontlie` – outbound: game results and schedule from the BALLDONTLIE API.
//! * `openrouter` – outbound: a chat model with tool calling for the question box.
//! * `espn` – outbound: game report links from ESPN's scoreboard feed.
pub mod balldontlie;
pub mod csv;
pub mod dataset;
pub mod espn;
pub mod http;
pub mod openrouter;
pub mod postgres;
