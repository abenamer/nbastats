//! Composition root (REAS-ARCH-001): the one place that names the concrete adapters and wires
//! them to the application. Everything below `application` sees only the ports.

use std::path::PathBuf;
use std::sync::Arc;

use clap::{Parser, Subcommand};
use tracing::{error, info, warn};
use tracing_subscriber::EnvFilter;

use nba_elo_simulator::adapters::balldontlie::{BalldontlieConfig, BalldontlieSource};
use nba_elo_simulator::adapters::dataset::HttpDatasetSource;
use nba_elo_simulator::adapters::http;
use nba_elo_simulator::adapters::postgres::PostgresStore;
use nba_elo_simulator::application::jobs;
use nba_elo_simulator::application::ports::{GameLog, GameStore, ImportLog, JobStore};
use nba_elo_simulator::application::scheduler::{self, SyncSchedule};
use nba_elo_simulator::application::state::App;
use nba_elo_simulator::application::{continuation, imports};
use nba_elo_simulator::config::{Config, APP_VERSION};
use nba_elo_simulator::domain::ids::SeasonYear;

#[derive(Parser)]
#[command(name = "nba-elo-simulator", version = APP_VERSION, about = "NBA Elo championship simulator")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Run migrations and start the web server (default)
    Serve,
    /// Run database migrations and exit
    Migrate,
    /// Import a CSV (optionally gzip-compressed) and rebuild derived tables
    Import {
        path: PathBuf,
        /// Re-import even if the identical file was imported before
        #[arg(long)]
        force: bool,
    },
    /// Download the configured source dataset and import it
    Refresh {
        #[arg(long)]
        force: bool,
    },
    /// Fetch a season's results from BALLDONTLIE and compute its Elo (needs BALLDONTLIE_API_KEY)
    SyncResults {
        /// Season end year, for example 2026 for 2025-26
        season: i32,
    },
    /// Container health check: GET /healthz on the bound address (exit 0 when healthy)
    Healthcheck,
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,sqlx=warn,tower_http=info"));
    let json = std::env::var("LOG_FORMAT")
        .map(|v| v == "json")
        .unwrap_or(false);
    if json {
        tracing_subscriber::fmt()
            .with_env_filter(filter)
            .json()
            .init();
    } else {
        tracing_subscriber::fmt().with_env_filter(filter).init();
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _ = dotenvy::dotenv();
    init_tracing();
    let cli = Cli::parse();
    if matches!(cli.command, Some(Command::Healthcheck)) {
        return healthcheck().await;
    }
    let config = Config::from_env()?;
    info!(version = APP_VERSION, database = %config.masked_database_url(), "starting");
    if let Some(threads) = config.worker_threads {
        rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .build_global()
            .ok();
    }

    // --- adapters ---
    let store = PostgresStore::connect_with_retry(
        &config.database_url,
        config.db_max_connections,
        config.db_connect_timeout,
    )
    .await?;
    store.migrate().await?;
    store.sync_reference().await?;
    imports::derived::backfill_curated_championships(&store).await?;
    store.sync_alignments().await?;
    info!("migrations applied and reference data synced");
    let source = HttpDatasetSource::new()?;
    // Composition root: the adapter is built here from plain configuration values.
    let results = match &config.balldontlie_api_key {
        Some(key) => Some(BalldontlieSource::new(BalldontlieConfig {
            api_key: key.clone(),
            base_url: config.balldontlie_base_url.clone(),
            requests_per_minute: config.balldontlie_requests_per_minute,
        })?),
        None => None,
    };

    match cli.command.unwrap_or(Command::Serve) {
        Command::Migrate => Ok(()),
        Command::Import { path, force } => {
            let outcome = imports::import_path(&store, &path, "cli", force).await?;
            println!("{}", serde_json::to_string_pretty(&outcome)?);
            Ok(())
        }
        Command::Refresh { force } => {
            let outcome = imports::refresh_from_url(
                &store,
                &source,
                &config.data_source_url,
                &config.data_dir,
                force,
            )
            .await?;
            println!("{}", serde_json::to_string_pretty(&outcome)?);
            Ok(())
        }
        Command::SyncResults { season } => {
            let season = SeasonYear::try_new(season)?;
            let results = results.ok_or_else(|| {
                anyhow::anyhow!(
                    "BALLDONTLIE_API_KEY must be set to sync results (see .env.example)"
                )
            })?;
            let progress = |p: nba_elo_simulator::application::ports::FetchProgress| {
                eprintln!(
                    "requests: {} games: {}{}",
                    p.requests,
                    p.games,
                    p.waiting
                        .map(|w| format!(" (next request in {}s)", w.as_secs()))
                        .unwrap_or_default()
                );
            };
            let zone: chrono_tz::Tz = config
                .results_sync_timezone
                .trim()
                .parse()
                .unwrap_or(chrono_tz::America::New_York);
            let today = chrono::Utc::now().with_timezone(&zone).date_naive();
            let outcome =
                continuation::sync_results(&store, &results, season, today, &progress).await?;
            println!("{}", serde_json::to_string_pretty(&outcome)?);
            Ok(())
        }
        Command::Serve => serve(config, store, source, results).await,
        Command::Healthcheck => unreachable!("handled before connecting"),
    }
}

async fn serve(
    config: Config,
    store: PostgresStore,
    source: HttpDatasetSource,
    results: Option<BalldontlieSource>,
) -> anyhow::Result<()> {
    std::fs::create_dir_all(&config.data_dir)?;
    let interrupted = store.mark_running_jobs_interrupted().await?;
    if interrupted > 0 {
        warn!(
            interrupted,
            "jobs left running by a previous process were marked interrupted"
        );
    }
    let stale = store.mark_stale_imports_failed().await?;
    if stale > 0 {
        warn!(
            stale,
            "imports left running by a previous process were marked failed"
        );
    }

    // First start: load the bundled dataset so the dashboard is useful immediately.
    if config.auto_import_bundled && store.game_count().await? == 0 {
        if config.bundled_dataset.exists() {
            info!(path = %config.bundled_dataset.display(), "database empty; importing bundled dataset");
            match imports::import_path(&store, &config.bundled_dataset, "bundled", false).await {
                Ok(o) => info!(inserted = o.games_inserted, "bundled dataset imported"),
                Err(e) => error!(error = %e, "bundled import failed"),
            }
        } else {
            warn!(path = %config.bundled_dataset.display(), "bundled dataset not found; import data from the Data Management page");
        }
    }

    // --- wiring ---
    let sync_schedule = match (&results, config.results_sync_enabled) {
        (Some(_), true) => Some(SyncSchedule::parse(
            &config.results_sync_time,
            &config.results_sync_timezone,
        )?),
        _ => None,
    };
    if results.is_some() {
        info!(
            scheduled = sync_schedule.is_some(),
            "BALLDONTLIE results sync available"
        );
    }
    let app = Arc::new(App::new(
        config.clone(),
        store.clone(),
        source,
        results,
        sync_schedule,
    ));
    let runner = tokio::spawn(jobs::runner(app.clone()));
    let sync = tokio::spawn(scheduler::run(app.clone()));
    let router = http::router(app.clone());
    let listener = tokio::net::TcpListener::bind(&config.bind_addr).await?;
    info!(addr = %config.bind_addr, "listening");
    let shutdown = app.shutdown.clone();
    axum::serve(listener, router)
        .with_graceful_shutdown(async move {
            shutdown_signal().await;
            info!("shutdown signal received; finishing in-flight requests");
            shutdown.cancel();
        })
        .await?;
    // Give the runner a moment to record an interrupted job before the pool closes.
    let _ = tokio::time::timeout(std::time::Duration::from_secs(10), runner).await;
    sync.abort();
    store.close().await;
    info!("stopped cleanly");
    Ok(())
}

/// Used by the container HEALTHCHECK so the runtime image needs no curl.
async fn healthcheck() -> anyhow::Result<()> {
    let bind = std::env::var("APP_BIND").unwrap_or_else(|_| "0.0.0.0:3000".to_string());
    let port = bind.rsplit(':').next().unwrap_or("3000");
    let url = format!("http://127.0.0.1:{port}/healthz");
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(4))
        .build()?;
    let resp = client.get(&url).send().await?;
    if resp.status().is_success() {
        Ok(())
    } else {
        anyhow::bail!("unhealthy: {} returned {}", url, resp.status())
    }
}

async fn shutdown_signal() {
    let ctrl_c = async {
        if let Err(e) = tokio::signal::ctrl_c().await {
            error!(error = %e, "ctrl-c handler failed");
        }
    };
    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut s) => {
                s.recv().await;
            }
            Err(e) => error!(error = %e, "SIGTERM handler failed"),
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
}
