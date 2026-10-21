---
type: Reference
title: Configuration
description: Environment variables read by the application and the Compose file.
tags: [configuration, environment]
generated: { by: "human:abenamer", at: "2026-10-21T17:30:00Z" }
sources:
  - id: env
    resource: /.env.example
    title: Example configuration
---

# Application variables

| Variable | Default | Function |
|----------|---------|----------|
| `DATABASE_URL` | none (required) | PostgreSQL connection string. |
| `APP_BIND` | `0.0.0.0:3000` | Address the server listens on inside the container. |
| `APP_DATA_DIR` | `./app-data` | Directory for downloaded datasets. The container uses `/var/lib/nba-app`. |
| `AUTO_IMPORT_BUNDLED` | `true` | Import the bundled dataset when the database is empty. |
| `BUNDLED_DATASET` | `./data/nba_elo_neil_paine.csv.gz` | Path of the bundled dataset. |
| `DATA_SOURCE_URL` | Neil Paine's `nba_elo.csv` on GitHub | URL used by refresh. |
| `MAX_ITERATIONS` | `5000000` | Upper limit for Monte Carlo iterations in one job. |
| `MAX_WORK_UNITS` | `4000000000` | Upper limit for the estimated work of one job (iterations × per-iteration work). |
| `ALLOWED_HOSTS` | `localhost,127.0.0.1,[::1]` | Host names accepted in the `Host` header. `*` accepts any. |
| `BALLDONTLIE_API_KEY` | none | API key for the results sync. When unset, the sync is unavailable. |
| `BALLDONTLIE_BASE_URL` | `https://api.balldontlie.io/v1` | Base URL of the BALLDONTLIE API. |
| `BALLDONTLIE_REQUESTS_PER_MINUTE` | `5` | Request pacing for the plan's quota. |
| `RESULTS_SYNC_ENABLED` | `true` | Daily sync of the current season (needs the API key). |
| `RESULTS_SYNC_TIME` | `04:00` | Wall-clock time of the daily sync, `HH:MM`. |
| `RESULTS_SYNC_TIMEZONE` | `America/New_York` | IANA time zone of the daily sync. Daylight saving time is applied. |
| `DB_MAX_CONNECTIONS` | `8` | Connection pool size. |
| `DB_CONNECT_TIMEOUT_SECS` | `120` | How long the start waits for PostgreSQL. |
| `SIM_THREADS` | number of CPUs | Rayon thread count. |
| `RUST_LOG` | `info,sqlx=warn` | Log filter. |
| `LOG_FORMAT` | `text` | `json` gives structured JSON lines. |

# Compose variables

| Variable | Default | Function |
|----------|---------|----------|
| `POSTGRES_USER` | `nba` | Database user. |
| `POSTGRES_PASSWORD` | none (required) | Database password. |
| `POSTGRES_DB` | `nba` | Database name. |
| `APP_HOST_BIND` | `127.0.0.1` | Host interface for the published port. |
| `APP_HOST_PORT` | `3000` | Host port for the dashboard. |

The database port is not published to the host. The application reaches it on the internal network `nba-internal`.
