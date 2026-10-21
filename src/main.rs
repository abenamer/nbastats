use std::path::PathBuf;
use std::sync::Arc;

use clap::{Parser, Subcommand};
use tracing::{error, info, warn};
use tracing_subscriber::EnvFilter;

use nba_elo_simulator::application::state::App;
use nba_elo_simulator::config::{Config, APP_VERSION};
use nba_elo_simulator::adapters::http as web;
use nba_elo_simulator::adapters::postgres as db;
use nba_elo_simulator::application::{imports, jobs};

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
    /// Container health check: GET /healthz on the bound address (exit 0 when healthy)
    Healthcheck,
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info,sqlx=warn,tower_http=info"));
    let json = std::env::var("LOG_FORMAT").map(|v| v == "json").unwrap_or(false);
    if json {
        tracing_subscriber::fmt().with_env_filter(filter).json().init();
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
        rayon::ThreadPoolBuilder::new().num_threads(threads).build_global().ok();
    }
    let pool = db::connect_with_retry(&config.database_url, config.db_max_connections, config.db_connect_timeout).await?;
    db::migrate(&pool).await?;
    db::teams::sync_reference(&pool).await?;
    info!("migrations applied and reference data synced");

    match cli.command.unwrap_or(Command::Serve) {
        Command::Migrate => Ok(()),
        Command::Import { path, force } => {
            let outcome = imports::import_path(&pool, &path, "cli", force).await?;
            println!("{}", serde_json::to_string_pretty(&outcome)?);
            Ok(())
        }
        Command::Refresh { force } => {
            let outcome = imports::refresh_from_url(&pool, &config.data_source_url, &config.data_dir, force).await?;
            println!("{}", serde_json::to_string_pretty(&outcome)?);
            Ok(())
        }
        Command::Serve => serve(config, pool).await,
        Command::Healthcheck => unreachable!("handled before connecting"),
    }
}

/// Used by the container HEALTHCHECK so the runtime image needs no curl.
async fn healthcheck() -> anyhow::Result<()> {
    let bind = std::env::var("APP_BIND").unwrap_or_else(|_| "0.0.0.0:3000".to_string());
    let port = bind.rsplit(':').next().unwrap_or("3000");
    let url = format!("http://127.0.0.1:{port}/healthz");
    let client = reqwest::Client::builder().timeout(std::time::Duration::from_secs(4)).build()?;
    let resp = client.get(&url).send().await?;
    if resp.status().is_success() {
        Ok(())
    } else {
        anyhow::bail!("unhealthy: {} returned {}", url, resp.status())
    }
}

async fn serve(config: Config, pool: db::Pool) -> anyhow::Result<()> {
    std::fs::create_dir_all(&config.data_dir)?;
    let interrupted = db::jobs::mark_interrupted(&pool).await?;
    if interrupted > 0 {
        warn!(interrupted, "jobs left running by a previous process were marked interrupted");
    }
    let stale = db::imports::mark_stale_failed(&pool).await?;
    if stale > 0 {
        warn!(stale, "imports left running by a previous process were marked failed");
    }
    let app = Arc::new(App::new(config.clone(), pool.clone()));

    // First start: load the bundled dataset so the dashboard is useful immediately.
    if config.auto_import_bundled && db::games::game_count(&pool).await? == 0 {
        if config.bundled_dataset.exists() {
            info!(path = %config.bundled_dataset.display(), "database empty; importing bundled dataset");
            match imports::import_path(&pool, &config.bundled_dataset, "bundled", false).await {
                Ok(o) => info!(inserted = o.games_inserted, "bundled dataset imported"),
                Err(e) => error!(error = %e, "bundled import failed"),
            }
        } else {
            warn!(path = %config.bundled_dataset.display(), "bundled dataset not found; import data from the Data Management page");
        }
    }

    let runner = tokio::spawn(jobs::runner(app.clone()));
    let router = web::router(app.clone());
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
    pool.close().await;
    info!("stopped cleanly");
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c().await.expect("ctrl-c handler");
    };
    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()).expect("sigterm handler").recv().await;
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
}
