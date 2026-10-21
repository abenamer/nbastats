---
type: Runbook
title: Develop and test
description: Build, run and test the application outside Docker.
tags: [development, cargo, test, runbook]
generated: { by: "human:abenamer", at: "2026-10-21T17:30:00Z" }
---

# Requirements

* Rust stable toolchain (1.80 or later).
* A PostgreSQL 16 or 17 server. Example with Docker:

      docker run -d --name nba-dev-pg -e POSTGRES_PASSWORD=devpass -e POSTGRES_USER=nba -e POSTGRES_DB=nba_dev -p 127.0.0.1:5433:5432 postgres:17-alpine

# Run

1. Create `.env` with:

       DATABASE_URL=postgres://nba:devpass@127.0.0.1:5433/nba_dev
       APP_BIND=127.0.0.1:3000
       APP_DATA_DIR=./app-data

2. Start the server:

       cargo run --release -- serve

# Test

Unit tests need no database:

    cargo test --lib

Integration tests need `DATABASE_URL`. Each test creates its own database and applies the migrations:

    cargo test

Lint:

    cargo clippy --all-targets

# Command line

| Command | Function |
|---------|----------|
| `serve` | Run migrations and start the server (default). |
| `migrate` | Run migrations and exit. |
| `import <file> [--force]` | Import a CSV file. |
| `refresh [--force]` | Download the configured source and import it. |
| `healthcheck` | Exit 0 when `/healthz` answers. Used by the container health check. |
