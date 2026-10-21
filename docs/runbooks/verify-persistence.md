---
type: Runbook
title: Verify persistence
description: Run the automated test that recreates the containers and checks that the data stays.
tags: [persistence, test, docker, runbook]
generated: { by: "human:abenamer", at: "2026-10-21T17:30:00Z" }
sources:
  - id: script
    resource: /scripts/verify-persistence.sh
    title: Persistence test script
---

# Procedure

    scripts/verify-persistence.sh [--build] [--keep]

The script:

1. Starts the stack and waits for the health check and the data import.
2. Writes a marker row (a named alpha scenario) through the API.
3. Runs `docker compose down` and `docker compose up -d`.
4. Runs `docker compose restart`. With `--build`, it also runs `docker compose up -d --build`.
5. Compares the game count, the championship count and the marker row before and after.
6. Checks that the two named volumes exist.

The last line is `PERSISTENCE TEST PASSED` or `PERSISTENCE TEST FAILED`. The script removes the marker row at the end unless you add `--keep`.

# Result for version 0.1.0

The test passed on 2026-10-21 with `--build`: 75,913 games and 22 championships before and after, marker row present.
