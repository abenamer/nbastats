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

| Component | Location | Function |
|-----------|----------|----------|
| Web server | `src/web` | Serves the pages and the JSON API on port 3000. |
| Engines | `src/simulation`, `src/playoffs`, `src/backtesting` | Pure functions. They do not touch the database. |
| Domain model | `src/models`, `src/reference` | Enumerated states and static reference data. |
| Import pipeline | `src/imports` | Parses CSV files and writes games into PostgreSQL. |
| Job runner | `src/jobs` | Runs simulations in the background and records progress. |
| Database layer | `src/db`, `migrations` | Connection pool, migrations and queries. |

See the [Elo rating model](/concepts/elo-model.md), the [alpha lottery engine](/concepts/alpha-engine.md), the [playoff simulator](/concepts/playoff-engine.md) and the [season forecast engine](/concepts/season-engine.md).

# Pages

| Page | Path | Content |
|------|------|---------|
| Dashboard | `/` | Title distribution, concentration and the equal-strength comparison. |
| Elo Explorer | `/elo` | Team ratings, Elo history charts, franchise trajectories and the matchup calculator. |
| Alpha Simulator | `/alpha` | Editable strengths, scenario save and load, exact validation. |
| Playoff Simulator | `/playoffs` | Historical bracket, exact and Monte Carlo probabilities, actual results. |
| Season Forecast | `/season` | Every team's probability to reach the play-in, the playoffs, each round and the title. |
| Dynasty Lab | `/dynasty` | Expected versus actual titles and significance estimates. |
| Validation | `/validation` | Backtest results, calibration plots, baselines and assumptions. |
| Data | `/data` | Imports, coverage, refresh controls, database health and version. |
| Jobs | `/jobs` | All simulation jobs with status, versions, retry and cancel. |

# Design rules

* Domain logic is separate from infrastructure. The engines are generic over a progress sink and a strength model. The compiler removes the abstraction cost.
* Every lifecycle is an enum with explicit transitions. Examples: job status, playoff result, series state, play-in stage.
* Monte Carlo runs are deterministic. The same seed and configuration give the same result on any number of threads.
* The application never invents Elo ratings. Seasons without data are shown as missing.
