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

# After an import

* The Data page shows the import with row counts and rejected rows.
* The coverage table shows each season, its game counts, and whether the postseason is complete.
* Seasons without playoff records show no derived champion unless the curated table has one.

# Limits

* The upload limit is 64 MB.
* Unknown team codes are reported in the import notes. Add the code to `src/reference.rs` to support a new team.
