---
type: Runbook
title: Back up and restore
description: Create, list, restore and verify PostgreSQL backups with the scripts in the scripts directory.
tags: [backup, restore, pg_dump, runbook]
generated: { by: "human:abenamer", at: "2026-10-21T17:30:00Z" }
sources:
  - id: scripts
    resource: /scripts/
    title: Operations scripts
---

# Location

Backups are files in `./backups` on the host. This directory is outside the PostgreSQL data volume. Copy the files to another disk or computer to keep them safe.

# Create a backup

    scripts/backup.sh [label]

The script runs `pg_dump` in custom format inside the database container. The file name is `nba_<UTC timestamp>[_label].dump`. The script adds the SHA-256 checksum to `backups/SHA256SUMS`.

# List backups

    scripts/list-backups.sh

# Restore a backup

    scripts/restore.sh backups/nba_20261021T170000Z.dump

The script:

1. Asks for confirmation. Add `--yes` to skip the question.
2. Stops the `nba-app` container.
3. Drops and recreates the `public` schema.
4. Runs `pg_restore`.
5. Verifies the database.
6. Starts the `nba-app` container.

**CAUTION:** A restore replaces all data in the database.

# Verify the database

    scripts/verify-database.sh

The script prints the row counts of the main tables. It fails when the migrations table is missing.

# Automate

Add a cron entry on the host, for example a daily backup at 03:15:

    15 3 * * * cd /path/to/nba-elo-simulator && scripts/backup.sh daily >> backups/backup.log 2>&1
