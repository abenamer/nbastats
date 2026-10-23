---
type: API
title: HTTP API
description: Pages and JSON endpoints served on port 3000.
tags: [api, http, json]
generated: { by: "human:abenamer", at: "2026-10-21T17:30:00Z" }
sources:
  - id: router
    resource: /src/web/mod.rs
    title: Router
---

# Pages

`/`, `/elo`, `/alpha`, `/playoffs`, `/season`, `/dynasty`, `/validation`, `/data`, `/jobs`. See the [application overview](/concepts/application.md).

# Health

| Method and path | Function |
|-----------------|----------|
| `GET /healthz` | 200 when the database answers, else 503. |
| `GET /api/health` | Version, uptime, database health, table counts, job counts. |

# Data

| Method and path | Function |
|-----------------|----------|
| `GET /api/seasons` | Season list with counts and dates. |
| `GET /api/seasons/{season}/teams` | Team-season rows: record, Elo milestones, seed, result. |
| `GET /api/seasons/{season}/elo-history?teams=GSW,CLE` | Elo after each game, and the `schedule` span (first and last game, last played game, games remaining). |
| `GET /api/seasons/{season}/bracket?home_advantage=100&scale=1` | Reconstructed bracket with exact probabilities. |
| `GET /api/records` | Every record table of the Records page, computed from the game log on each call. |
| `GET /api/franchises` | Franchise list with uniform colours (`colors.primary`, `colors.secondary`; defunct franchises get a neutral pair). |
| `GET /api/franchises/{key}/elo?from=2003&to=2025` | Elo after every game of a franchise, its season rows, its championships (won and lost) and its colours. |
| `GET /api/matchup?elo_a=1700&elo_b=1600&home=a` | Game and series probabilities. |
| `GET /api/exact/alpha?franchises=30&seasons=22&threshold=4` | Exact equal-strength occupancy probabilities. |
| `GET /api/dashboard?from=2004&to=2025` | Title distribution and concentration. |
| `GET /api/dynasty?franchise=GSW&from=2015&to=2022&alpha=3` | Dynasty report and concentration comparison. |
| `GET /api/data/imports` | Import history. |
| `GET /api/data/coverage` | Season coverage, missing seasons, the next season without data, an example of the results layout, the configured results source (`null` when none) and the daily sync schedule with its next run (`null` when disabled). |
| `POST /api/data/refresh` | Download and import the source. Body: `{"force": false}`. |
| `POST /api/data/upload` | Multipart upload. Fields: `file`, `force`. |
| `POST /api/data/project` | Preseason projection with computed Elo. Body: `{"season": 2026}`. Refused when the season has imported games. |
| `POST /api/data/results` | Multipart upload in the results layout with computed Elo. Fields: `file`, `season`. |
| `DELETE /api/data/seasons/{season}` | Remove a season that holds only computed games or no games. Status 400 for a season with imported games. |
| `POST /api/data/sync` | Start a background sync of a season from BALLDONTLIE; for a season with archive games, only the games after the archive are added. Body: `{"season": 2026}`. Returns at once. Status 400 when `BALLDONTLIE_API_KEY` is not set, 409 when a sync is already running. |
| `GET /api/data/sync/status` | State of the last or current sync: `idle`, `fetching` (season, trigger, requests, games, seconds until the next request), `done` (outcome) or `failed` (error). |

# Jobs

| Method and path | Function |
|-----------------|----------|
| `POST /api/jobs/alpha` | Queue an Engine A run. Body: `AlphaParams` plus optional `name`. |
| `POST /api/jobs/playoff` | Queue an Engine B run. Body: `PlayoffJob` plus optional `name`. |
| `POST /api/jobs/season` | Queue an Engine C run. Body: `SeasonParams` plus optional `name`. |
| `POST /api/jobs/backtest` | Queue a backtest. Body: `BacktestParams` plus optional `name`. |
| `GET /api/jobs?kind=alpha&limit=50` | List jobs without results. |
| `GET /api/jobs/{id}` | One job with its result. |
| `POST /api/jobs/{id}/retry` | Re-queue a failed, interrupted or cancelled job. |
| `POST /api/jobs/{id}/cancel` | Cancel a queued or running job. |
| `POST /api/jobs/{id}/name` | Rename. Body: `{"name": "..."}`. |
| `DELETE /api/jobs/{id}` | Delete a job that is not running. |
| `GET /api/scenarios/{kind}` | Named, completed jobs of a kind. |
| `GET /api/backtests/latest` | The latest completed backtest. |

# Example

    curl -s -X POST http://localhost:3000/api/jobs/alpha \
      -H 'Content-Type: application/json' \
      -d '{"name":"equal","franchises":["A","B","C"],"alphas":[1,1,1],"seasons":10,"iterations":100000,"threshold":3,"seed":1}'

Poll `GET /api/jobs/{id}` until `status` is `completed`.

# Errors

Errors return JSON `{"error": "message"}` with status 400 (bad input), 404 (not found) or 500 (internal, details only in the log).
