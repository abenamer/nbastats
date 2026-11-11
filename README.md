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

## Not a betting tool

This project forecasts games, playoffs and titles and measures how good the forecasts are. It is not a way to make money, and the record it keeps says so. Over six seasons and about 7,400 games, the model picks the winner of about 66 percent of regular-season games. The sportsbook's closing line picks about 68 percent, and the sportsbook was the closer forecaster in every one of those seasons. A flat $100 on the model's favourite in every game since 2020 would be down about $32,000. The same $100 on FanDuel's own favourite would be down about $30,000. Almost all of that is the sportsbook's margin, about 4 cents on every dollar; with the margin taken out, both land within a rounding error of break-even. Where the model and the sportsbook disagree about the favourite, the model is right 41 percent of the time, and a stake that grows with the size of the disagreement loses faster. The Bets page runs these portfolios so the result can be seen rather than taken on trust.

The sportsbook wins not because its mathematics is better. Its line is the average of thousands of bettors, some of whom know the lineups, the late scratches and the minutes limits before tip-off, and the book moves the line when the bettors it has learned to respect place a bet. By game time the line knows everything the model knows and what happened that afternoon. About a third of NBA games are coin flips to everyone, the sportsbook included. See [Market tracking](docs/concepts/market-tracking.md) and the literature review in `research/`.

The numbers are as of November 2026 and move with the record.

## Quick start

1. Copy the example configuration and set a password:

       cp .env.example .env
       # edit .env: set POSTGRES_PASSWORD (the other values have working defaults)

2. Build and start:

       docker compose up -d --build

3. Open <http://localhost:3000>. To open the dashboard from your other devices, see [Reach the dashboard over Tailscale](docs/runbooks/tailscale.md).

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
| Model vs Market | Every game the sportsbook priced and the model forecast, scored after the result, and which of the two has been the more accurate. |
| Bets | The same record read as a betting portfolio: a pot staked on every game under seven strategies, at the posted or the fair prices. It shows the loss. |
| Top 100 | The 100 players since 1976-77 with the largest career worth, in tiers of chance: each player starts from what the box score of his career says, and his teams' results in the games he played and the games he missed move the number. Computed again after each data pull. A noisy measure, and the page says how noisy. |
| Dynasty Lab | Expected versus actual titles, significance under equal strength, a ticket lottery and historical Elo; the franchise's strength in Elo against the league mean, the best other team and the worst, and the titles read backwards. |
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

* **Elo**: `P(A beats B) = 1 / (1 + 10^((E_B − E_A)/400))`, home bonus 100 points in the rating update and the playoffs, a bonus fitted per season for regular-season forecasts, optional scale and K-factor. See [Elo rating model](docs/concepts/elo-model.md).
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
