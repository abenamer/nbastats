---
type: Reference
title: Simulation jobs
description: Job lifecycle, versioning and retry rules.
tags: [jobs, lifecycle, versions]
generated: { by: "human:abenamer", at: "2026-10-21T17:30:00Z" }
sources:
  - id: runner
    resource: /src/jobs/mod.rs
    title: Job runner
  - id: states
    resource: /src/models/mod.rs
    title: Job state machine
---

# Lifecycle

```
queued --claim--> running --complete--> completed
                  running --fail-----> failed
                  running --interrupt-> interrupted
                  running --cancel---> cancelled
queued --cancel--> cancelled
failed | interrupted | cancelled --retry--> queued
```

`JobStatus::transition` in the code is the only way to move between states. A completed job cannot be re-run. Create a new job instead.

# Execution

* The runner takes one queued job at a time (`FOR UPDATE SKIP LOCKED`).
* The engine runs on the blocking thread pool with Rayon. The HTTP server stays responsive.
* Progress is written to the database every 500 ms.
* When the application receives SIGTERM, the running job stops at the next chunk and is marked `interrupted`. Completed results are already stored.
* At start, jobs left in `running` by a previous process are marked `interrupted`.

# Versions

Each job stores the application version and the model version of its engine:

| Kind | Model version |
|------|---------------|
| alpha | `alpha-lottery-1.0` |
| playoff | `elo-playoff-1.0` |
| season | `elo-season-1.5` |
| backtest | `backtest-1.1` |

A new application version does not change stored results. Compare the versions when you compare scenarios.

# Limits

Iterations are limited by `MAX_ITERATIONS` (default 5,000,000). Bad parameters are rejected with status 400 before the job is created.
