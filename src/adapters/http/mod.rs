//! HTTP layer: router, JSON API, server-rendered pages and error mapping.

pub mod api;
pub mod error;
pub mod pages;

use std::sync::Arc;

use axum::extract::DefaultBodyLimit;
use axum::routing::{get, post};
use axum::Router;
use tower_http::compression::CompressionLayer;
use tower_http::services::ServeDir;
use tower_http::trace::TraceLayer;

use crate::application::state::App;

pub type State = Arc<App>;

pub fn router(app: State) -> Router {
    let api = Router::new()
        .route("/health", get(api::health))
        .route("/seasons", get(api::seasons))
        .route("/seasons/{season}/teams", get(api::season_teams))
        .route("/seasons/{season}/elo-history", get(api::season_elo_history))
        .route("/seasons/{season}/bracket", get(api::season_bracket))
        .route("/franchises", get(api::franchises))
        .route("/franchises/{key}/elo", get(api::franchise_elo))
        .route("/matchup", get(api::matchup))
        .route("/exact/alpha", get(api::exact_alpha))
        .route("/dashboard", get(api::dashboard))
        .route("/dynasty", get(api::dynasty))
        .route("/jobs", get(api::list_jobs))
        .route("/jobs/alpha", post(api::submit_alpha))
        .route("/jobs/playoff", post(api::submit_playoff))
        .route("/jobs/season", post(api::submit_season))
        .route("/jobs/backtest", post(api::submit_backtest))
        .route("/jobs/{id}", get(api::get_job).delete(api::delete_job))
        .route("/jobs/{id}/retry", post(api::retry_job))
        .route("/jobs/{id}/cancel", post(api::cancel_job))
        .route("/jobs/{id}/name", post(api::rename_job))
        .route("/scenarios/{kind}", get(api::scenarios))
        .route("/backtests/latest", get(api::latest_backtest))
        .route("/data/imports", get(api::imports))
        .route("/data/refresh", post(api::refresh))
        .route("/data/upload", post(api::upload))
        .route("/data/coverage", get(api::coverage));

    Router::new()
        .route("/", get(pages::dashboard))
        .route("/elo", get(pages::elo))
        .route("/alpha", get(pages::alpha))
        .route("/playoffs", get(pages::playoffs))
        .route("/season", get(pages::season))
        .route("/dynasty", get(pages::dynasty))
        .route("/validation", get(pages::validation))
        .route("/data", get(pages::data))
        .route("/jobs", get(pages::jobs))
        .route("/healthz", get(api::healthz))
        .nest("/api", api)
        .nest_service("/static", ServeDir::new("static"))
        .layer(DefaultBodyLimit::max(64 * 1024 * 1024))
        .layer(CompressionLayer::new())
        .layer(TraceLayer::new_for_http())
        .with_state(app)
}

