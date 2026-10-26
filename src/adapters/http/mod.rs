//! HTTP layer: router, JSON API, server-rendered pages and error mapping.

pub mod api;
pub mod error;
pub mod guard;
pub mod pages;

use std::sync::{Arc, OnceLock};

use axum::extract::DefaultBodyLimit;
use axum::http::{header, HeaderValue};
use axum::routing::{delete, get, post};
use axum::{middleware, Router};
use tower_http::compression::CompressionLayer;
use tower_http::services::ServeDir;
use tower_http::set_header::{SetResponseHeader, SetResponseHeaderLayer};
use tower_http::trace::TraceLayer;

use crate::application::state::App;

pub type State = Arc<App>;

pub fn router(app: State) -> Router {
    let api = Router::new()
        .route("/health", get(api::health))
        .route("/seasons", get(api::seasons))
        .route("/seasons/{season}/teams", get(api::season_teams))
        .route(
            "/seasons/{season}/elo-history",
            get(api::season_elo_history),
        )
        .route("/seasons/{season}/bracket", get(api::season_bracket))
        .route(
            "/seasons/{season}/home-advantage",
            get(api::season_home_advantage),
        )
        .route("/franchises", get(api::franchises))
        .route("/records", get(api::records))
        .route("/records/groups", get(api::records_groups))
        .route("/records/facets", get(api::records_facets))
        .route("/reports", get(api::game_reports))
        .route("/franchises/{key}/elo", get(api::franchise_elo))
        .route("/matchup", get(api::matchup))
        .route("/exact/alpha", get(api::exact_alpha))
        .route("/strength", get(api::strength))
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
        .route("/data/results", post(api::upload_results))
        .route("/data/project", post(api::project_season))
        .route("/data/sync", post(api::sync_results))
        .route("/data/sync/status", get(api::sync_status))
        .route("/data/seasons/{season}", delete(api::remove_season))
        .route("/data/coverage", get(api::coverage))
        .route("/data/players", get(api::players_data))
        .route("/data/players/status", get(api::players_status))
        .route("/data/players/backfill", post(api::players_backfill))
        .route(
            "/data/players/backfill-all",
            post(api::players_backfill_all),
        )
        .route("/data/players/injuries", post(api::players_injury_report))
        .route("/data/players/load", post(api::players_load_all))
        .route("/data/players/rosters", post(api::players_refresh_active))
        .route("/players", get(api::players_search))
        .route("/players/{id}", get(api::player))
        .route("/players/{id}/games", get(api::player_games))
        .route("/games/{id}/box-score", get(api::game_box_score))
        .route("/injuries", get(api::injuries))
        .route("/ask/status", get(api::ask_status))
        .route(
            "/ask",
            post(api::ask).layer(DefaultBodyLimit::max(16 * 1024)),
        );

    Router::new()
        .route("/", get(pages::dashboard))
        .route("/elo", get(pages::elo))
        .route("/franchise", get(pages::franchise))
        .route("/records", get(pages::records))
        .route("/alpha", get(pages::alpha))
        .route("/playoffs", get(pages::playoffs))
        .route("/season", get(pages::season))
        .route("/dynasty", get(pages::dynasty))
        .route("/validation", get(pages::validation))
        .route("/data", get(pages::data))
        .route("/jobs", get(pages::jobs))
        .route("/healthz", get(api::healthz))
        .nest("/api", api)
        // Browsers revalidate static files on every load (Last-Modified from ServeDir),
        // and the pages version their asset URLs, so a rebuild never pairs an old script with a new one.
        .nest_service(
            "/static",
            SetResponseHeader::overriding(
                ServeDir::new("static"),
                header::CACHE_CONTROL,
                HeaderValue::from_static("no-cache"),
            ),
        )
        .layer(DefaultBodyLimit::max(64 * 1024 * 1024))
        .layer(CompressionLayer::new())
        .layer(SetResponseHeaderLayer::if_not_present(
            header::CONTENT_SECURITY_POLICY,
            HeaderValue::from_static(
                "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; \
                 connect-src 'self'; frame-ancestors 'none'; base-uri 'none'; form-action 'self'",
            ),
        ))
        .layer(SetResponseHeaderLayer::if_not_present(
            header::X_CONTENT_TYPE_OPTIONS,
            HeaderValue::from_static("nosniff"),
        ))
        .layer(SetResponseHeaderLayer::if_not_present(
            header::X_FRAME_OPTIONS,
            HeaderValue::from_static("DENY"),
        ))
        .layer(SetResponseHeaderLayer::if_not_present(
            header::REFERRER_POLICY,
            HeaderValue::from_static("same-origin"),
        ))
        .layer(middleware::from_fn_with_state(app.clone(), guard::browser_request_guard))
        .layer(TraceLayer::new_for_http())
        .with_state(app)
}

/// A short hash of the served scripts and stylesheet, computed once at first use. Pages append
/// it to the static URLs (`?v=...`), so a changed file is a new URL for the browser.
pub fn asset_version() -> &'static str {
    static VERSION: OnceLock<String> = OnceLock::new();
    VERSION.get_or_init(|| {
        let mut paths: Vec<_> = std::fs::read_dir("static/js")
            .map(|d| {
                d.filter_map(|e| e.ok().map(|e| e.path()))
                    .filter(|p| p.extension().is_some_and(|x| x == "js"))
                    .collect()
            })
            .unwrap_or_default();
        paths.push(std::path::PathBuf::from("static/css/app.css"));
        paths.sort();
        let mut bytes = Vec::new();
        for p in paths {
            if let Ok(b) = std::fs::read(&p) {
                bytes.extend_from_slice(p.to_string_lossy().as_bytes());
                bytes.extend_from_slice(&b);
            }
        }
        if bytes.is_empty() {
            return crate::config::APP_VERSION.to_string();
        }
        crate::application::imports::sha256_hex(&bytes)[..12].to_string()
    })
}
