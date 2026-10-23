---
type: Runbook
title: Import and refresh data
description: Load a CSV game log from a file or download the latest version from the configured source.
tags: [import, data, runbook]
generated: { by: "human:abenamer", at: "2026-10-21T17:30:00Z" }
---

# Refresh from the source

1. Open the Data page: `http://localhost:3000/data`.
2. Click **Download & import latest**.

The application downloads `DATA_SOURCE_URL`, stores the file in the application data volume, and imports it. An identical file (same SHA-256) is skipped. Select **Force re-import** to import it again.

Command line alternative:

    docker compose exec nba-app nba-elo-simulator refresh

# Upload a file

1. Open the Data page.
2. Select a CSV file in the `nba_elo.csv` or `nbaallelo.csv` layout. Gzip files are accepted.
3. Click **Upload CSV**.

Command line alternative for a file on the host:

    docker compose cp ./my_file.csv nba-app:/tmp/my_file.csv
    docker compose exec nba-app nba-elo-simulator import /tmp/my_file.csv

# Add a season after the archive

The archive ends with 2024-25. For a later season, the application computes the Elo ratings itself (see [Elo continuation](../concepts/elo-continuation.md)).

## Project a season before it starts

1. Open the Data page.
2. In **Seasons beyond the Elo dataset**, enter the season end year (2026 for 2025-26).
3. Click **Project preseason (synthetic schedule)**.

The application inserts the previous season's regular-season matchups one year later, with no scores, and the reverted ratings of the previous season. The Season Forecast page can then forecast that season.

## Sync results from BALLDONTLIE

1. Create a free account at `https://www.balldontlie.io` and copy the API key.
2. Set `BALLDONTLIE_API_KEY` in `.env`. Set `BALLDONTLIE_REQUESTS_PER_MINUTE` to your plan's quota (free: 5).
3. Restart the application: `docker compose up -d`.
4. On the Data page, enter the season end year and click **Sync results from BALLDONTLIE**.

The sync runs in the background. The line under the button shows its progress: requests made, games received, and the seconds until the next request when the application waits for the API quota. The first sync of a season fetches the whole schedule, because the season forecast simulates the games that are not played yet; on the free tier it takes about three minutes. Later syncs ask only for the days from the last played game to today (one or two requests), so a postponed game keeps its old date until the season is removed and synced again. When the sync ends, the page shows the outcome and refreshes the coverage table. Only one sync runs at a time; the daily sync shows the same line while it runs.

## Daily automatic sync

When `BALLDONTLIE_API_KEY` is set, the application syncs the current season every day at `RESULTS_SYNC_TIME` in `RESULTS_SYNC_TIMEZONE` (default 04:00 America/New_York, after the last West Coast game). The current season is the one that ends next year from September to December, and the one that ends this year from January to August.

* At start, the application runs the sync at once if no sync has completed since the last scheduled time (for example after the computer was off at 04:00).
* When the previous season is not in the store yet (the first run after a new season starts), the application syncs that season first, so the ratings chain from the archive. It tries this once per run: when the previous season yields nothing either, the run stops and the log says so.
* The Data page shows the next run time. The import history shows each run with the source `balldontlie`.
* A season whose archive games already cover everything the source has stores nothing; the log says so and the current season still runs.
* Set `RESULTS_SYNC_ENABLED=false` to turn the daily sync off and keep the button.

Command line alternative, for an external scheduler:

    docker compose exec nba-app nba-elo-simulator sync-results 2026

## Import results and the schedule

1. Prepare a CSV file with this header: `date,home,away,home_score,away_score,neutral,playoff`.
2. Use the team codes of the Elo Explorer (OKC, GSW, ...). Leave both scores empty for a game that is not played yet. Set `neutral` or `playoff` to 1 when they apply.
3. On the Data page, enter the season end year, select the file and click **Upload results / schedule CSV for that season**.

Example:

    date,home,away,home_score,away_score,neutral,playoff
    2025-10-21,OKC,HOU,125,124,0,0
    2025-10-22,LAL,GSW,,,0,0

Each upload replaces the season's games that did not come from the archive, then computes every rating of the season again. Rows with a tied score, one missing score, or an unknown team code are rejected and reported.

A projection is refused for a season that has archive games. A results sync or upload of such a season keeps every archive game and adds only the games the archive does not have (for 2024-25: the play-in and the playoffs). A source or file with no new games stores nothing.

## Complete 2024-25

The archive stops after the 2024-25 regular season. To add its play-in and playoffs:

1. On the Data page, enter 2025 and click **Sync results from BALLDONTLIE**.
2. When it completes, enter 2026 and sync again, so 2025-26 starts from the completed ratings.

The coverage table then shows 2024-25 as complete, with the derived champion, and the "Results from" column as archive plus results.

## Remove a season without archive games

A projection, or a sync of a season the source does not have yet, can be removed. In the coverage table, click **Remove** on the row. The application deletes the season's games, standings, derived champion and forecasts. The button is not shown for seasons with archive games, and those are never removed.

## Recompute every rating

The application computes every rating at an archive import and, once, at the first start after the upgrade to compute first (the log says so; 80 seasons take about 15 seconds). To run it by hand:

    docker compose exec nba-app nba-elo-simulator recompute

The ratings of every season are written in one transaction. When the computation fails, no rating changes: an archive import is then marked failed with the reason, although its games are stored, and the start continues with the ratings it has. Fix the cause (the log names it) and run the command above, or restart the application.

An archive import (download or upload) does not run while a results sync runs, and a sync does not start during an import: the second one answers "already running" (status 409). Try again when the first has finished.

# After an import

* The Data page shows the import with row counts and rejected rows.
* The coverage table shows each season, its game counts, where its results come from (archive, results, projection), how many games are not played yet, and whether the postseason is complete.
* Seasons without playoff records show no derived champion unless the curated table has one.

# Limits

* The upload limit is 64 MB.
* Unknown team codes are reported in the import notes. Add the code to `src/reference.rs` to support a new team.
