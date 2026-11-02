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

`.env.example` lists every variable below, grouped by purpose, with its default; secrets are empty. Under Compose, `compose.yaml` passes each application variable from `.env` into the container, and builds `DATABASE_URL` from the `POSTGRES_*` values when it is not set.

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
| `BALLDONTLIE_API_KEY` | none | API key for the results sync and the player data. When unset, both are unavailable. Box scores, rosters and injury reports need the ALL-STAR tier; see [Player data](/concepts/player-data.md). |
| `BALLDONTLIE_BASE_URL` | `https://api.balldontlie.io/v1` | Base URL of the BALLDONTLIE API. |
| `BALLDONTLIE_REQUESTS_PER_MINUTE` | `5` | Request pacing for the plan's quota: 5 on the free tier, 60 on ALL-STAR, 600 on GOAT (the API's pricing page, read 2026-10-26). |
| `RESULTS_SYNC_ENABLED` | `true` | Daily sync of the current season (needs the API key). |
| `RESULTS_SYNC_TIME` | `04:00` | Wall-clock time of the daily sync, `HH:MM`. |
| `RESULTS_SYNC_TIMEZONE` | `America/New_York` | IANA time zone of the daily sync's clock time. Daylight saving time is applied. The dates the application and its commands read as today, and the games' dates, are the league's (Eastern time) whatever this zone. |
| `PLAYER_SYNC_ENABLED` | `true` | After the daily results sync, fetch the current season's box scores, today's injury report and the rosters; see [Player data](/concepts/player-data.md). Needs the daily sync. |
| `PLAYER_BACKFILL_FROM` | none | A season end year (1947 for every season). When set, a background loop backfills the box scores of every season from it to the current one, season by season, until nothing is left. It retries after a failure and resumes after a restart; then the daily sync takes over. Needs the API key. |
| `OPENROUTER_API_KEY` | none | API key for the [question box](/concepts/question-box.md). When unset, the box is off. |
| `OPENROUTER_MODEL` | `openai/gpt-4o-mini` | The model the box asks through OpenRouter; it must support tool calling. |
| `OPENROUTER_BASE_URL` | `https://openrouter.ai/api/v1` | Base URL of the OpenAI-compatible chat API. |
| `GAME_REPORTS_ENABLED` | `true` | Look up game report links (the AP recaps ESPN hosts) for the games the Records page shows, see [Historical data](/concepts/historical-data.md), and read the night's sportsbook lines for the Tonight panel when there is no Odds API key or its read fails, see [ADR 0006](/decisions/0006-market-lines-from-espn-scoreboard.md). |
| `MARKET_TRACKING_ENABLED` | `true` | With a market source (The Odds API or the ESPN feed), save the model's numbers and the sportsbook's lines in the last eleven minutes before each game's start, to score the model against the market; see [Market tracking](/concepts/market-tracking.md). |
| `ESPN_BASE_URL` | `https://site.api.espn.com` | Base URL of ESPN's scoreboard feed, for the reports and the lines. |
| `ODDS_API_KEY` | none | API key for The Odds API. When set, the Tonight panel and the market tracking read the chosen sportsbook's lines from it, with ESPN as the fallback; see [ADR 0009](/decisions/0009-market-lines-from-the-odds-api.md). A read costs three credits (one per market); a game night of tracking about 20 (one read for the night and one per tip-off time), a month in season 550 to 600, more than the free 500. |
| `ODDS_API_BOOKMAKER` | `fanduel` | The sportsbook whose lines The Odds API is asked for, by its key at the service (`fanduel`, `draftkings`, `betmgm`, ...): lowercase letters, digits and underscores. A key the service does not know makes every read fail (the log says so); with ESPN off, the tracker then tries again at every ten-minute look, each a billed read, so check the key when the log warns. |
| `ODDS_API_BASE_URL` | `https://api.the-odds-api.com` | Base URL of The Odds API. |
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
