---
type: Architecture Decision
title: Ports at the store, dataset and HTTP boundaries
description: Why the application defines twelve port traits and one concrete store adapter.
tags: [decision, architecture, reas]
generated: { by: "human:abenamer", at: "2026-10-21T19:00:00Z" }
---

# ADR 0001 — Ports at the store, dataset and HTTP boundaries

## Context

REAS favours ports and adapters for human-maintained code and asks three questions for private projects: who reads and maintains the code, which external systems it uses, and how long it must run. This application is a self-hosted research tool. It uses PostgreSQL, the file system and one HTTPS source. It must run for years and receive data updates.

## Decision

* The application defines twelve ports in `src/application/ports.rs`: `GameLog`, `GameStore`, `ImportLog`, `JobStore`, `AnalysisStore`, `ReportStore`, `PlayerStore`, `DatasetSource`, `ResultsSource`, `LanguageModel`, `ReportSource`, `PlayerSource`.
* One adapter type, `PostgresStore`, implements the seven store ports. `HttpDatasetSource` implements `DatasetSource`. `BalldontlieSource` implements `ResultsSource` (added 2026-10-21 when the archive no longer covered the current season). `OpenRouterModel` implements `LanguageModel` (added 2026-10-23 for the question box, [ADR 0005](0005-question-box-via-openrouter.md)). `EspnReportSource` implements `ReportSource` and `PostgresStore` also implements `ReportStore` (added 2026-10-23 for the game report links). `BalldontlieSource` also implements `PlayerSource` and `PostgresStore` implements `PlayerStore` (added 2026-10-25 for the player data: one adapter, two ports, because the same API and key serve both).
* `App` holds the concrete adapters. Use cases that benefit from fakes (import, job runner, refresh) take the ports as generic parameters. HTTP handlers call the ports through `app.store`.
* `main.rs` is the composition root.

## Consequences

* An operation is traceable: HTTP handler → use case → port method → `PostgresStore` SQL.
* Tests can replace the store with an in-memory fake without a database.
* `App` is not generic, so Axum handlers stay simple. A second store adapter would need a change in `App`.

## Alternatives

* A generic repository trait was rejected; REAS-ARCH-004 warns against it.
* Making `App` generic over every port was rejected; it adds type parameters to every handler without a second adapter in sight.

## Open question for the maintainer

Answer the REAS-ARCH-003 questions and record them here: readers and maintainers, external systems, lifetime and rate of change.
