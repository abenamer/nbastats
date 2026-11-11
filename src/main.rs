//! Composition root (REAS-ARCH-001): the one place that names the concrete adapters and wires
//! them to the application. Everything below `application` sees only the ports.

use std::path::PathBuf;
use std::sync::Arc;

use clap::{Parser, Subcommand};
use tracing::{error, info, warn};
use tracing_subscriber::EnvFilter;

use nba_elo_simulator::adapters::balldontlie::{BalldontlieConfig, BalldontlieSource};
use nba_elo_simulator::adapters::dataset::HttpDatasetSource;
use nba_elo_simulator::adapters::espn::EspnScoreboard;
use nba_elo_simulator::adapters::http;
use nba_elo_simulator::adapters::odds_api::{OddsApi, OddsApiConfig};
use nba_elo_simulator::adapters::openrouter::{OpenRouterConfig, OpenRouterKeys, OpenRouterModel};
use nba_elo_simulator::adapters::postgres::PostgresStore;
use nba_elo_simulator::application::jobs;
use nba_elo_simulator::application::ports::LanguageModel;
use nba_elo_simulator::application::ports::{GameLog, GameStore, ImportLog, JobStore};
use nba_elo_simulator::application::scheduler::{self, SyncSchedule};
use nba_elo_simulator::application::state::{App, Outbound};
use nba_elo_simulator::application::{continuation, imports, players, recompute};
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
    /// Rate every stored game again from the first season (compute first)
    Recompute,
    /// Fetch a season's results from BALLDONTLIE and compute its Elo (needs BALLDONTLIE_API_KEY)
    SyncResults {
        /// Season end year, for example 2026 for 2025-26
        season: i32,
    },
    /// Fetch the box scores of every unfetched date of a season from BALLDONTLIE (needs
    /// BALLDONTLIE_API_KEY and a tier with box scores); resumes where a previous run stopped
    BackfillPlayers {
        /// Season end year, for example 2026 for 2025-26
        season: i32,
        /// Also store today's injury report
        #[arg(long)]
        injuries: bool,
        /// Also load the source's whole player list (every player, active or not)
        #[arg(long)]
        all_players: bool,
        /// Also refresh the current rosters (which players are active, and where)
        #[arg(long)]
        rosters: bool,
        /// Backfill every season from the given one to the current season, newest first
        #[arg(long)]
        every_season: bool,
    },
    /// Backtest YAXEL, the emergence term, over the stored seasons and print the report
    /// (read-only: no rating or page changes)
    YaxelBacktest {
        /// First season end year, for example 2010 for 2009-10
        #[arg(long)]
        from: i32,
        /// Last season end year
        #[arg(long)]
        to: i32,
        /// The early-season cut: games in which neither team has played more than this many
        #[arg(long, default_value_t = 30)]
        through: u32,
        /// The new-player cut: a player with no record in a stored season before this one
        /// counts when he played at least this many minutes in his team's previous game
        #[arg(long, default_value_t = 24.0)]
        new_minutes: f64,
        /// Also write the full report as JSON to this path
        #[arg(long)]
        json: Option<std::path::PathBuf>,
    },
    /// Rank the players since 1976-77 by career worth, from the box score of their career
    /// and their teams' games with them and without them, and print the report (read-only:
    /// no rating or page changes)
    CareerWorth {
        /// The games as a regular a player needs to be ranked
        #[arg(long)]
        min_games: Option<u32>,
        /// How many times the ranking is drawn
        #[arg(long)]
        draws: Option<u32>,
        /// The seed of the draws
        #[arg(long)]
        seed: Option<u64>,
        /// How many players the list holds
        #[arg(long)]
        listed: Option<usize>,
        /// Also write the full report as JSON to this path
        #[arg(long)]
        json: Option<std::path::PathBuf>,
    },
    /// Backfill the market record from The Odds API's history: for each night in the range
    /// with stored games and no snapshot, the sportsbook's lines as they stood before the
    /// tip-off, beside the stored pre-game Elo (needs ODDS_API_KEY on a paid plan; costs
    /// ten credits per market and read, one per night's listing)
    MarketBackfill {
        /// First date, YYYY-MM-DD (the league's date)
        #[arg(long)]
        from: chrono::NaiveDate,
        /// Last date, YYYY-MM-DD, inclusive
        #[arg(long)]
        to: chrono::NaiveDate,
        /// Read the lines before each tip-off time of the night, as the live capture does,
        /// instead of once before the night's first tip-off
        #[arg(long)]
        per_tip_off: bool,
        /// The markets to read, comma-separated: h2h (the moneyline), spreads, totals
        #[arg(long, default_value = "h2h")]
        markets: String,
        /// List the nights and the estimated cost without reading anything
        #[arg(long)]
        dry_run: bool,
        /// Stop before a request that would take the run's credits, by the service's rates,
        /// past this many; without it the run stops only when the service's credits run out
        #[arg(long)]
        max_credits: Option<u64>,
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
    if store.has_imported_ratings().await? {
        info!("games still carry the archive's ratings; computing the application's own ratings for every season");
        // The store is unchanged when this fails (one transaction); the archive's ratings
        // serve until the next start or `recompute` succeeds.
        if let Err(e) = recompute::recompute_all(&store).await {
            error!(error = %e, "recompute failed; the archive's ratings stay in use until the next start or `recompute` succeeds");
        }
    }
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
    // The question box runs on the configured key, on a key the management key mints, or
    // on both: the configured key answers until the first minted one is installed.
    let keys = match &config.openrouter_management_key {
        Some(key) => Some(OpenRouterKeys::new(
            key.clone(),
            config.openrouter_base_url.clone(),
        )?),
        None => None,
    };
    let model_config = OpenRouterConfig {
        api_key: config.openrouter_api_key.clone().unwrap_or_default(),
        base_url: config.openrouter_base_url.clone(),
        model: config.openrouter_model.clone(),
    };
    let model = match (&config.openrouter_api_key, &keys) {
        (Some(_), _) => Some(OpenRouterModel::new(model_config)?),
        (None, Some(_)) => Some(OpenRouterModel::awaiting_key(model_config)?),
        (None, None) => None,
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
        Command::Recompute => {
            let outcome = recompute::recompute_all(&store).await?;
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
            let today = nba_elo_simulator::domain::calendar::league_date(chrono::Utc::now());
            let outcome =
                continuation::sync_results(&store, &results, season, today, &progress).await?;
            println!("{}", serde_json::to_string_pretty(&outcome)?);
            Ok(())
        }
        Command::BackfillPlayers {
            season,
            injuries,
            all_players,
            rosters,
            every_season,
        } => {
            let season = SeasonYear::try_new(season)?;
            let results = results.ok_or_else(|| {
                anyhow::anyhow!(
                    "BALLDONTLIE_API_KEY must be set to fetch box scores (see .env.example)"
                )
            })?;
            let progress = |p: players::PlayerProgress| {
                eprintln!(
                    "{}dates: {}/{}{} lines: {} requests: {}{}",
                    p.season
                        .map(|s| format!(
                            "season {s} ({}/{}) ",
                            p.seasons_done + 1,
                            p.seasons_total
                        ))
                        .unwrap_or_default(),
                    p.dates_done,
                    p.dates_total,
                    p.current_date
                        .map(|d| format!(" ({d})"))
                        .unwrap_or_default(),
                    p.lines,
                    p.fetch.requests,
                    p.fetch
                        .waiting
                        .map(|w| format!(" (next request in {}s)", w.as_secs()))
                        .unwrap_or_default()
                );
            };
            let today = nba_elo_simulator::domain::calendar::league_date(chrono::Utc::now());
            let mut outcome = if every_season {
                let to = scheduler::current_season(today);
                players::backfill_all(&store, &results, season, to, &progress).await?
            } else {
                players::backfill_season(&store, &results, season, &progress).await?
            };
            if injuries {
                let lines =
                    players::fetch_injury_report(&store, &results, today, &progress).await?;
                outcome.injury_report = Some((today, lines));
            }
            if all_players {
                outcome.players_loaded =
                    Some(players::load_all_players(&store, &results, &progress).await?);
            }
            if rosters {
                outcome.active_players =
                    Some(players::refresh_active_players(&store, &results, &progress).await?);
            }
            println!("{}", serde_json::to_string_pretty(&outcome)?);
            Ok(())
        }
        Command::YaxelBacktest {
            from,
            to,
            through,
            new_minutes,
            json,
        } => {
            use nba_elo_simulator::application::state::{App, Outbound};
            use nba_elo_simulator::application::yaxel;
            let from = SeasonYear::try_new(from)?;
            let to = SeasonYear::try_new(to)?;
            anyhow::ensure!(from <= to, "--from must not be after --to");
            anyhow::ensure!(
                new_minutes.is_finite() && (0.0..=48.0).contains(&new_minutes),
                "--new-minutes must be between 0 and 48"
            );
            let app = App::new(
                config,
                store,
                source,
                Outbound {
                    results: None,
                    sync_schedule: None,
                    model: None,
                    keys: None,
                    reports: None,
                    odds: None,
                },
            );
            let grid = nba_elo_simulator::domain::yaxel::Grid::default();
            let progress = |line: &str| eprintln!("{line}");
            let answer =
                yaxel::backtest(&app, from, to, &grid, through, new_minutes, &progress).await?;
            if let Some(path) = json {
                std::fs::write(&path, serde_json::to_string_pretty(&answer)?)?;
                eprintln!("report written to {}", path.display());
            }
            println!("{}", yaxel::render(&answer));
            Ok(())
        }
        Command::CareerWorth {
            min_games,
            draws,
            seed,
            listed,
            json,
        } => {
            use nba_elo_simulator::application::career;
            use nba_elo_simulator::domain::career::Settings;
            let d = Settings::default();
            let settings = Settings {
                min_games: min_games.unwrap_or(d.min_games),
                draws: draws.unwrap_or(d.draws),
                seed: seed.unwrap_or(d.seed),
                listed: listed.unwrap_or(d.listed),
            };
            career::check(&settings).map_err(|m| anyhow::anyhow!(m))?;
            let app = App::new(
                config,
                store,
                source,
                Outbound {
                    results: None,
                    sync_schedule: None,
                    model: None,
                    keys: None,
                    reports: None,
                    odds: None,
                },
            );
            let report = career::compute(&app, settings).await?;
            if let Some(path) = json {
                std::fs::write(&path, serde_json::to_string_pretty(&report)?)?;
                eprintln!("report written to {}", path.display());
            }
            println!("{}", career::render(&report));
            Ok(())
        }
        Command::MarketBackfill {
            from,
            to,
            per_tip_off,
            markets,
            dry_run,
            max_credits,
        } => {
            use nba_elo_simulator::application::backfill::{self, Plan};
            use nba_elo_simulator::application::ports::OddsMarket;
            use nba_elo_simulator::application::state::{App, Outbound};
            anyhow::ensure!(from <= to, "--from must not be after --to");
            // Each market once, in the order given: a repeated one would be billed again.
            let mut parsed: Vec<OddsMarket> = Vec::new();
            for w in markets.split(',').filter(|w| !w.trim().is_empty()) {
                let m = OddsMarket::parse(w).ok_or_else(|| {
                    anyhow::anyhow!("--markets must name h2h, spreads or totals; not {w:?}")
                })?;
                if !parsed.contains(&m) {
                    parsed.push(m);
                }
            }
            let markets = parsed;
            anyhow::ensure!(
                !markets.is_empty(),
                "--markets must name at least one market"
            );
            let key = config
                .odds_api_key
                .clone()
                .ok_or_else(|| anyhow::anyhow!("ODDS_API_KEY must be set to read the history"))?;
            if !plain_or_local(&config.odds_api_base_url) {
                warn!("ODDS_API_BASE_URL is not https: the API key travels unencrypted in the query string");
            }
            let odds = OddsApi::new(OddsApiConfig {
                api_key: key,
                base_url: config.odds_api_base_url.clone(),
                bookmaker: config.odds_api_bookmaker.clone(),
            })?;
            let app = App::new(
                config,
                store,
                source,
                Outbound {
                    results: None,
                    sync_schedule: None,
                    model: None,
                    keys: None,
                    reports: None,
                    odds: Some(odds),
                },
            );
            let history = app
                .odds
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("The Odds API adapter was not kept"))?;
            let plan = Plan {
                per_tip_off,
                markets,
            };
            let progress = |line: &str| eprintln!("{line}");
            let outcome = backfill::backfill(
                &app,
                history,
                from,
                to,
                &plan,
                dry_run,
                max_credits,
                &progress,
            )
            .await?;
            println!("{}", serde_json::to_string_pretty(&outcome)?);
            Ok(())
        }
        Command::Serve => serve(config, store, source, results, model, keys).await,
        Command::Healthcheck => unreachable!("handled before connecting"),
    }
}

/// Whether a base URL carries a key safely: `https`, or plain `http` to this computer.
fn plain_or_local(url: &str) -> bool {
    url.starts_with("https://")
        || url.starts_with("http://127.0.0.1")
        || url.starts_with("http://localhost")
}

async fn serve(
    config: Config,
    store: PostgresStore,
    source: HttpDatasetSource,
    results: Option<BalldontlieSource>,
    model: Option<OpenRouterModel>,
    keys: Option<OpenRouterKeys>,
) -> anyhow::Result<()> {
    std::fs::create_dir_all(&config.data_dir)?;
    let reports = if config.game_reports_enabled {
        Some(EspnScoreboard::new(config.espn_base_url.clone())?)
    } else {
        None
    };
    let odds = match &config.odds_api_key {
        Some(key) => Some(OddsApi::new(OddsApiConfig {
            api_key: key.clone(),
            base_url: config.odds_api_base_url.clone(),
            bookmaker: config.odds_api_bookmaker.clone(),
        })?),
        None => None,
    };
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
        if !plain_or_local(&config.balldontlie_base_url) {
            warn!("BALLDONTLIE_BASE_URL is not https: the API key travels unencrypted");
        }
    }
    let app = Arc::new(App::new(
        config.clone(),
        store.clone(),
        source,
        Outbound {
            results,
            sync_schedule,
            model,
            keys,
            reports,
            odds,
        },
    ));
    if let Some(r) = &app.reports {
        info!(base_url = r.base_url(), "game report lookups enabled");
    }
    if let Some(o) = &app.odds {
        info!(
            base_url = o.base_url(),
            bookmaker = o.bookmaker(),
            "market lines from The Odds API"
        );
        if !plain_or_local(o.base_url()) {
            warn!("ODDS_API_BASE_URL is not https: the API key travels unencrypted in the query string");
        }
    }
    if let Some(m) = &app.model {
        info!(
            model = m.model_name(),
            base_url = m.base_url(),
            "question box enabled"
        );
        let url = m.base_url();
        if !plain_or_local(url) {
            warn!("OPENROUTER_BASE_URL is not https: the API key and the questions travel unencrypted");
        }
    }
    if app.keys.is_some() {
        info!(
            every_hours = config.openrouter_key_rotation_hours,
            limit_credits = config.openrouter_key_limit,
            key_name = %config.openrouter_key_name,
            "question box key rotation on: the key is minted at start and replaced on the schedule and on a rejection"
        );
    }
    let runner = tokio::spawn(jobs::runner(app.clone()));
    let sync = tokio::spawn(scheduler::run(app.clone()));
    let tracking = tokio::spawn(nba_elo_simulator::application::tracking::run(app.clone()));
    let mut rotation = tokio::spawn(nba_elo_simulator::application::keys::run(app.clone()));
    let backfill = match (config.player_backfill_from, app.results.is_some()) {
        (Some(from), true) => {
            info!(%from, "box-score backfill of every season scheduled (PLAYER_BACKFILL_FROM)");
            let app = app.clone();
            Some(tokio::spawn(async move {
                if let Some(source) = app.results.as_ref() {
                    players::backfill_loop(app.clone(), source, from).await;
                }
            }))
        }
        (Some(_), false) => {
            warn!("PLAYER_BACKFILL_FROM is set but BALLDONTLIE_API_KEY is not; no backfill");
            None
        }
        _ => None,
    };
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
    tracking.abort();
    // The rotation loop ends on the shutdown signal; a minting in flight is given a moment to
    // finish, so the key it mints is known to the stop and deleted. A loop still busy is
    // stopped, and its end awaited, before the keys are deleted, so the two never run at
    // once; a key minted after that is retired at the next start.
    if tokio::time::timeout(std::time::Duration::from_secs(5), &mut rotation)
        .await
        .is_err()
    {
        rotation.abort();
        let _ = (&mut rotation).await;
    }
    nba_elo_simulator::application::keys::stop(&app).await;
    if let Some(b) = backfill {
        b.abort();
    }
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
