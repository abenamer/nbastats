---
type: Architecture Decision
title: Ports at the store, dataset and HTTP boundaries
description: Why the application defines six port traits and one concrete store adapter.
tags: [decision, architecture, reas]
generated: { by: "human:abenamer", at: "2026-10-21T19:00:00Z" }
---

# ADR 0001 — Ports at the store, dataset and HTTP boundaries

## Context

REAS favours ports and adapters for human-maintained code and asks three questions for private projects: who reads and maintains the code, which external systems it uses, and how long it must run. This application is a self-hosted research tool. It uses PostgreSQL, the file system and one HTTPS source. It must run for years and receive data updates.

## Decision

* The application defines six ports in `src/application/ports.rs`: `GameLog`, `GameStore`, `ImportLog`, `JobStore`, `AnalysisStore`, `DatasetSource`.
* One adapter type, `PostgresStore`, implements the five store ports. `HttpDatasetSource` implements `DatasetSource`.
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
