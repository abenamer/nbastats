---
type: Runbook
title: Start the application
description: First start of the application with Docker Compose.
tags: [docker, start, runbook]
generated: { by: "human:abenamer", at: "2026-10-21T17:30:00Z" }
---

# Before you start

* Install Docker Engine 24 or later with the Compose plugin.
* Make sure that port 3000 on your computer is free.

# Procedure

1. Copy the example configuration:

       cp .env.example .env

2. Open `.env`. Set `POSTGRES_PASSWORD` to a new password. Set the same password in `DATABASE_URL`.

   **WARNING:** The default password in `.env.example` is public. Do not keep it.

3. Build the image and start the services:

       docker compose up -d --build

   The first build takes several minutes.

4. Check the services:

       docker compose ps

   Both services must show `healthy`.

5. Open the dashboard in a browser:

       http://localhost:3000

# What happens at the first start

* The application waits for PostgreSQL. It retries the connection for up to 120 seconds.
* The application applies the SQL migrations.
* The application imports the bundled dataset when the `games` table is empty. This takes about 15 seconds. The Data page shows the import.

# Build behind an inspecting proxy

When outbound TLS on your network goes through an inspecting proxy, the build cannot download Rust crates. Give the build the proxy's CA certificate as a build secret:

    docker build --secret id=cacert,src=/path/to/ca.pem -t nba-elo-simulator:latest .
    docker compose up -d

Without the secret the build uses the normal system trust store.

# Logs

Show the application log:

    docker compose logs -f nba-app

# Stop

Stop the services and keep all data:

    docker compose down

Start again:

    docker compose up -d

See the [persistence model](/concepts/persistence.md) for the full list of safe commands.
