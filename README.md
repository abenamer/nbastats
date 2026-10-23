# NBA Elo Championship Simulator

A self-hosted research tool for NBA championship probabilities. It runs on your computer with Docker Compose. It keeps its data in PostgreSQL 17. It is written in Rust (Axum, Tokio, SQLx, Rayon).

The application answers these questions:

1. What is the probability that one franchise wins K or more titles in N seasons?
2. What is the probability that at least one of 30 franchises wins four, five or more titles?
3. How do unequal franchise strengths (alpha values) change these distributions?
4. Do historical Elo ratings explain championship concentration since 2003-04?
5. How well does Elo predict playoff results and champions?
6. Is a dynasty such as the Golden State Warriors statistically unusual when you use season-specific team strength?

The project follows the [REAS engineering standard](https://github.com/abenamer/reas). The documentation is an [Open Knowledge Format bundle](docs/index.md) that targets ASD-STE100 Simplified Technical English. Full STE compliance is not verified; see the [self-audit](docs/self-audit.md).

## Quick start

1. Copy the example configuration and set a password:

       cp .env.example .env
       # edit .env: set POSTGRES_PASSWORD (the other values have working defaults)

2. Build and start:

       docker compose up -d --build

3. Open <http://localhost:3000>.

The first start imports the bundled game log (75,913 games, 1946-47 to 2024-25). This takes about 15 seconds.

## Commands

| Task | Command |
|------|---------|
| Start or upgrade | `docker compose up -d --build` |
| Status | `docker compose ps` |
| Logs | `docker compose logs -f nba-app` |
| Stop (keeps data) | `docker compose down` |
| Start again | `docker compose up -d` |
| Back up | `scripts/backup.sh` |
| List backups | `scripts/list-backups.sh` |
| Restore | `scripts/restore.sh backups/<file>.dump` |
| Verify the database | `scripts/verify-database.sh` |
| Persistence test | `scripts/verify-persistence.sh --build` |

**CAUTION:** `docker compose down -v` deletes the named volumes `nba_postgres_data` and `nba_application_data`. All games, jobs and results are lost. Do not use it for routine upgrades. See [Persistence model](docs/concepts/persistence.md).

## Pages

| Page | Function |
|------|----------|
| Dashboard | Title distribution, concentration (Gini, HHI, top-5 share) and the equal-strength comparison. |
| Elo Explorer | Team ratings per season, Elo history charts, franchise trajectories, matchup calculator. |
| Alpha Simulator | Engine A. Editable strengths, seasons, iterations, threshold K, seed, exact validation, saved scenarios. |
| Playoff Simulator | Engine B. Historical bracket, Elo ratings, exact and Monte Carlo probabilities, actual results. |
| Season Forecast | Engine C. For every team: play-in, playoffs, second round, conference finals, Finals and title probability from a chosen date. |
| Dynasty Lab | Expected versus actual titles, significance under equal strength, persistent alpha and historical Elo. |
| Validation | Backtests from 2003-04, calibration plots, Elo against baselines, rolling parameter tuning, assumptions. |
| Data | Imports, season coverage, refresh and upload, database health, application version. |
| Jobs | All simulation jobs with status, versions, retry, cancel and delete. |

## Persistence

Containers are disposable. The named volumes hold the data:

| Volume | Content |
|--------|---------|
| `nba_postgres_data` | PostgreSQL data directory. |
| `nba_application_data` | Downloaded datasets and file state. |

These commands keep all data: `stop`, `start`, `restart`, `down` + `up -d`, `up -d --build`, and a host reboot. The database port is not published to the host.

Backups go to `./backups`, outside the data volume. See [Back up and restore](docs/runbooks/backup-restore.md).

## Data

The bundled game log is Neil Paine's archive of FiveThirtyEight's NBA Elo data. The application also imports FiveThirtyEight's older `nbaallelo.csv` layout. Imports are idempotent: unchanged games are skipped, changed games are updated, bad rows are counted and reported. The application never invents Elo ratings. See [Historical data](docs/concepts/historical-data.md).

Franchise identity follows relocations and renamings (SEA → OKC, NJN → BRK). Playoff brackets are rebuilt from the game records with the seeding rules of each era.

## Models

* **Elo**: `P(A beats B) = 1 / (1 + 10^((E_B − E_A)/400))`, home bonus 100 points, optional scale and K-factor. See [Elo rating model](docs/concepts/elo-model.md).
* **Engine A**: alpha-weighted lottery with an exact occupancy check. For 30 equal franchises over 22 seasons, P(some franchise wins 4+) = 16.05%. See [Alpha lottery engine](docs/concepts/alpha-engine.md).
* **Engine B**: 16-team bracket, exact dynamic program plus Monte Carlo. See [Playoff simulator](docs/concepts/playoff-engine.md).
* **Engine C**: full-season simulation with the play-in state machine. See [Season forecast engine](docs/concepts/season-engine.md).
* **Backtesting**: Brier score, log loss, calibration, baselines, chronological rolling tuning. See [Backtesting method](docs/concepts/backtesting.md).

## Development

    cargo test --lib                 # unit tests, no database
    DATABASE_URL=... cargo test      # integration tests, each in a fresh database
    cargo clippy --all-targets
    cargo run --release -- serve

See [Develop and test](docs/runbooks/develop.md).

## Project layout

    Cargo.toml  Dockerfile  compose.yaml  .env.example  CLAUDE.md  AGENTS.md
    migrations/            SQL migrations (SQLx)
    src/
      main.rs              composition root and CLI: serve, migrate, import, refresh, healthcheck
      config.rs            environment configuration
      domain/              pure: ids, models and state machines, reference data, elo, simulation (Engine A),
                           playoffs (Engines B and C), backtesting, validation
      application/         ports, use cases (imports, jobs, dashboard, dynasty), shared state
      adapters/            postgres (PostgresStore), http (Axum), csv (parsers), dataset (download)
    templates/             Askama templates
    static/                CSS, JavaScript, bundled Chart.js (no internet needed at runtime)
    data/                  bundled dataset
    scripts/               backup, restore, verify, documentation checker
    tests/                 PostgreSQL integration tests and documentation checker tests
    docs/                  OKF knowledge bundle (concepts, runbooks, reference, decisions, self-audit)
    research/              literature reviews with their notes and scripts, and the papers they cite

## Checks

    cargo fmt --all -- --check
    cargo clippy --all-targets -- -D warnings
    cargo test                                   # integration tests need DATABASE_URL
    python scripts/check_docs.py
    python -m unittest discover -s tests/docs -t .

## Licence

MIT. Chart.js is MIT (see `static/js/CHARTJS-LICENSE.md`). The game data is from FiveThirtyEight (CC BY 4.0) via Neil Paine's archive.
