---
type: Concept
title: Persistence model
description: Which state is durable, where it lives and which Docker commands keep it.
tags: [docker, volumes, persistence]
generated: { by: "human:abenamer", at: "2026-10-21T17:30:00Z" }
sources:
  - id: compose
    resource: /compose.yaml
    title: Compose file
---

# Durable state

| Volume | Mount | Content |
|--------|-------|---------|
| `nba_postgres_data` | `/var/lib/postgresql/data` in `nba-postgres` | All tables: games, seasons, jobs, forecasts, imports, players, box-score lines, injury reports. |
| `nba_application_data` | `/var/lib/nba-app` in `nba-app` | Downloaded datasets and other file state. |

The volumes have explicit names. A different project directory does not create a second database.

# Containers are disposable

A container holds the program, not the data. Docker can delete and recreate a container at any time. The named volumes stay.

# Safe commands

These commands keep all data:

* `docker compose stop` and `docker compose start`
* `docker compose restart`
* `docker compose down` then `docker compose up -d`
* `docker compose up -d --build`
* A host reboot. Docker restarts the containers because of `restart: unless-stopped`.

# Dangerous command

**CAUTION:** `docker compose down -v` deletes the named volumes and all data. Do not use it for routine upgrades. Make a [backup](/runbooks/backup-restore.md) first if you must use it.

# Jobs across restarts

A job keeps its configuration and seed in the database. When the application stops during a job, the job is marked `interrupted`. Completed results stay. You can retry the job from the Jobs page.
