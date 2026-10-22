---
type: Concept
title: Application overview
description: What the NBA Elo Championship Simulator does and how its parts fit together.
tags: [overview, architecture]
generated: { by: "human:abenamer", at: "2026-10-21T17:30:00Z" }
sources:
  - id: repo
    resource: /
    title: Application source code
---

# Purpose

The application is a research tool for NBA championship probabilities. It runs on your computer. It keeps its data in PostgreSQL. It answers these questions:

1. What is the probability that one franchise wins K or more titles in N seasons?
2. What is the probability that at least one of 30 franchises wins four, five or more titles?
3. How do unequal franchise strengths (alpha values) change these distributions?
4. Do historical Elo ratings explain championship concentration since 2003-04?
5. How well does Elo predict playoff results and champions?
6. Is a dynasty, for example the Golden State Warriors, statistically unusual when you use season-specific team strength?

# Components

The code follows the REAS ports-and-adapters layout. The path of an operation is visible: inbound adapter → use case → port → concrete adapter.

| Layer | Location | Content |
|-------|----------|---------|
| Domain | `src/domain` | Identifier newtypes, models and state machines, reference data, Elo mathematics, Engines A, B and C, backtesting. No Axum, SQLx or HTTP. |
| Application | `src/application` | Ports (`ports.rs`), use cases (imports, jobs, dashboard, dynasty) and shared state (`state.rs`). |
| Adapters | `src/adapters` | `postgres::PostgresStore` (store ports), `dataset::HttpDatasetSource` (download port), `http` (Axum router, API, pages), `csv` (parsers). |
| Composition root | `src/main.rs` | Creates the adapters, wires `App`, starts the runner and the server. |

See the [Elo rating model](/concepts/elo-model.md), the [alpha lottery engine](/concepts/alpha-engine.md), the [playoff simulator](/concepts/playoff-engine.md) and the [season forecast engine](/concepts/season-engine.md).

# Pages

| Page | Path | Content |
|------|------|---------|
| Dashboard | `/` | Title distribution, concentration and the equal-strength comparison. |
| Elo Explorer | `/elo` | Team ratings, Elo history charts, franchise trajectories and the matchup calculator. |
| Records | `/records` | Elo records: the highest and lowest ratings ever, by decade, by league (NBA/BAA and ABA) and by league size; the best and worst conferences and divisions by mean pre-playoff rating; the best and weakest champions, the best teams without a title, season-end extremes, the biggest rises and falls in a season, and the biggest single-game swings. |
| Franchise History | `/franchise` | A franchise's Elo after every game across all its seasons, drawn in the franchise's uniform colours (the darker colour as the line, the lighter as the area under it), with stars at NBA titles, hollow circles at lost Finals, rating bands, and an optional second franchise for comparison. |
| Alpha Simulator | `/alpha` | Editable strengths, scenario save and load, exact validation. |
| Playoff Simulator | `/playoffs` | Historical bracket, exact and Monte Carlo probabilities, actual results. |
| Season Forecast | `/season` | Every team's probability to reach the play-in, the playoffs, each round and the title. |
| Dynasty Lab | `/dynasty` | Expected versus actual titles and significance estimates. |
| Validation | `/validation` | Backtest results, calibration plots, baselines and assumptions. |
| Data | `/data` | Imports, coverage, refresh controls, database health and version. |
| Jobs | `/jobs` | All simulation jobs with status, versions, retry and cancel. |

# Design rules

* Domain logic is separate from infrastructure. The engines are generic over a progress sink and a strength model. The compiler removes the abstraction cost.
* Identifiers are newtypes: `TeamId`, `FranchiseId` and a validated `SeasonYear`.
* Every lifecycle is an enum with explicit transitions. Examples: job status, playoff result, series state, play-in stage.
* Monte Carlo runs are deterministic. The same seed and configuration give the same result on any number of threads.
* The application never invents Elo ratings. Seasons without data are shown as missing.
