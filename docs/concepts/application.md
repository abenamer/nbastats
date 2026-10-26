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
| Application | `src/application` | Ports (`ports.rs`), use cases (imports, recompute, continuation, jobs, dashboard, dynasty, ask) and shared state (`state.rs`). |
| Adapters | `src/adapters` | `postgres::PostgresStore` (store ports), `dataset::HttpDatasetSource` (download port), `balldontlie::BalldontlieSource` (results port), `openrouter::OpenRouterModel` (language model port), `http` (Axum router, API, pages), `csv` (parsers). |
| Composition root | `src/main.rs` | Creates the adapters, wires `App`, starts the runner and the server. |

See the [Elo rating model](/concepts/elo-model.md), the [alpha lottery engine](/concepts/alpha-engine.md), the [playoff simulator](/concepts/playoff-engine.md) and the [season forecast engine](/concepts/season-engine.md).

# Pages

Every table sorts on a header click: numbers as numbers, text as text, blanks last; a second click reverses the order. Static files (scripts, stylesheet) carry a content hash in their URL and are served with `Cache-Control: no-cache`, so a rebuilt application never runs an old script with a new one.

| Page | Path | Content |
|------|------|---------|
| Dashboard | `/` | The [question box](/concepts/question-box.md) (a plain-language question answered from the stored data, with the lookups it made), title distribution, concentration and the equal-strength comparison. |
| Elo Explorer | `/elo` | Team ratings (a click on a team code highlights it in the chart), Elo history charts (the season chart shows a dot for a team with one game, a line from two games on, and a shaded strip for the games not played yet), franchise trajectories and the matchup calculator. |
| Records | `/records` | Elo records (every game score links to its game report when one is found, see [Historical data](/concepts/historical-data.md)): the highest and lowest ratings ever (each line says whether the rating was reached in the regular season or the playoffs, with the regular-season and the playoff record); by era (a new era when a league's team count changes or a rule period starts; the rule periods, named after the shot clock, the 3-point line, zone defense and the hand-check ban, group the eras), by decade, by league (NBA/BAA and ABA) and by league size; East against West by mean pre-playoff rating for every season since 1946-47 (with the champion and its conference), the [parity profile](/concepts/parity.md) of every season (the champion, the finalists, the final four and the playoff field above the league mean in Elo points and standard deviations, with the champion's rank), and the best and worst divisions (a team counts when its season reached the playoffs and it played at least 80 percent of the median game count of that season, so lockout and early short schedules count and a team that folded does not); the best and weakest champions (season peak and the rating after the clinching game), the best teams without a title, season-end extremes, the biggest rises and falls in a season, the biggest single-game swings, and the matchups: the 20 played games with the highest combined pre-game rating of the two teams (greatest matchups) and the weakest game of each era, NBA/BAA and ABA apart (clankers by era); the hardest and the easiest road to the title of each era (a champion's mean opponent rating over every game, each phase weighing by its games); the records by collective bargaining agreement (best and weakest team-season, titles and champions, title HHI and Elo spread of each period, with what each agreement changed; see [Rule periods and bargaining periods](/concepts/periods.md)); and the [facets](/concepts/era-builder.md) at the top of the page, which replay every table over the seasons of the picked eras (bargaining period, rule period, league size, league, decade, competition format, computed dynasty era, curated player era, curated themed era; within a dimension any, across dimensions all) under a banner that names the picks, the seasons and what changed, with a group panel that computes one row of records per combination of dimensions within the same facets. Champions come from the championships table, so the curated early seasons are included. |
| Franchise History | `/franchise` | A franchise's Elo after every game across all its seasons, drawn in the franchise's uniform colours (the darker colour as the line, the lighter as the area under it), with stars at NBA titles, hollow circles at lost Finals, rating bands, and an optional second franchise for comparison. |
| Alpha Simulator | `/alpha` | The [alpha lottery](/concepts/alpha-engine.md): how often a title run happens by chance under assumed franchise strengths. Editable strengths, a franchise of interest, a fading-strength mode, scenario save, load and compare, exact validation, and the record read backwards into an implied alpha and an implied strength against the real playoff fields; the page explains its inputs and results, and the runbook [Run an alpha scenario](/runbooks/alpha-scenario.md) walks through three questions. |
| Playoff Simulator | `/playoffs` | Historical bracket, exact and Monte Carlo probabilities, actual results. |
| Season Forecast | `/season` | Every team's probability to reach the play-in, the playoffs, each round and the title. On request it prices in the injury report or players marked out, with their worth drawn from its uncertainty if asked. |
| Players | `/players` | Every player's worth in Elo from the games his team played without him, his box-score worth (BPM 2.0, VORP, PER) on the same scale, and the latest injury report priced for the forecast. |
| Dynasty Lab | `/dynasty` | Expected versus actual titles and significance estimates. |
| Validation | `/validation` | Backtest results, calibration plots, baselines and assumptions. |
| Data | `/data` | Imports, coverage, refresh controls, database health and version; the player data (box-score backfill, injury report, player search). |
| Jobs | `/jobs` | All simulation jobs with status, versions, retry and cancel. |

# Design rules

* Domain logic is separate from infrastructure. The engines are generic over a progress sink and a strength model. The compiler removes the abstraction cost.
* Identifiers are newtypes: `TeamId`, `FranchiseId` and a validated `SeasonYear`.
* Every lifecycle is an enum with explicit transitions. Examples: job status, playoff result, series state, play-in stage.
* Monte Carlo runs are deterministic. The same seed and configuration give the same result on any number of threads.
* The application never invents Elo ratings. Seasons without data are shown as missing.
