---
type: Architecture Decision
title: Runtime enums for persisted lifecycles
description: Why job status, series state and play-in stage are runtime enums, not typestate.
tags: [decision, state-machines, reas]
generated: { by: "human:abenamer", at: "2026-10-21T19:00:00Z" }
---

# ADR 0002 — Runtime enums for persisted lifecycles

## Context

REAS-STATE-002 recommends typestate for compile-time lifecycles and runtime enums for externally loaded or persisted state.

## Decision

* `JobStatus` is a runtime enum with `transition(event)`. Jobs are loaded from PostgreSQL, so their state is external.
* `SeriesState` and `PlayInState` are runtime enums. Their transitions happen inside hot simulation loops; a `match` compiles to a jump table.
* No typestate is used.

## Consequences

* The store enforces transitions with conditional updates (REAS-STATE-003).
* Illegal transitions are errors at run time (`InvalidTransition`), covered by unit tests.
