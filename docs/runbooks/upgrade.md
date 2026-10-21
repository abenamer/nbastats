---
type: Runbook
title: Upgrade the application
description: Rebuild the application image from new source code and keep all data.
tags: [docker, upgrade, runbook]
generated: { by: "human:abenamer", at: "2026-10-21T17:30:00Z" }
---

# Procedure

1. Make a backup. See [back up and restore](/runbooks/backup-restore.md).
2. Get the new source code (for example `git pull`).
3. Rebuild the image and replace the running containers:

       docker compose up -d --build

   Docker replaces the `nba-app` container. The named volumes stay. New migrations run at start.

4. Check the application version on the Data page or with:

       curl -s http://localhost:3000/api/health

5. Check the services:

       docker compose ps

# Rules

* Do not run `docker compose down -v`. It deletes the volumes.
* Migrations only add to the schema. They never drop tables with data.
* Stored results keep their application version and model version. A new version does not change old results.

# Rollback

1. Check out the previous source code.
2. Run `docker compose up -d --build` again.
3. If a new migration is incompatible, restore the backup from step 1.
