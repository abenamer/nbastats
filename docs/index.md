---
okf_version: "0.2"
---
# NBA Elo Championship Simulator — knowledge bundle

This bundle describes the application, its models, and its operation. The text follows ASD-STE100 Simplified Technical English. Each concept is one file with YAML front matter. Links between concepts are bundle-relative.

# Standard

This project follows the [REAS engineering standard](https://github.com/abenamer/reas).

* [Self-audit](self-audit.md) - Evidence and limits of REAS compliance.
* [Project terminology](terminology.md) - Project-specific technical terms.
* [STE review](ste-review.md) - Status and procedure of the Simplified Technical English review.

# Concepts

* [Application overview](concepts/application.md) - What the application does and how its parts fit together.
* [Elo rating model](concepts/elo-model.md) - The game and series probability formulas and their parameters.
* [Alpha lottery engine](concepts/alpha-engine.md) - Engine A: what the alpha lottery answers, what its inputs mean, how to read its results, and its exact check.
* [Playoff simulator](concepts/playoff-engine.md) - Engine B: bracket reconstruction, exact probabilities and Monte Carlo.
* [Season forecast engine](concepts/season-engine.md) - Engine C: full-season simulation with play-in and playoffs.
* [NBA Cup forecast](concepts/nba-cup.md) - How the season forecast simulates the NBA Cup from the schedule, with the groups derived from the group nights and a projected knockout bracket.
* [Backtesting method](concepts/backtesting.md) - How forecasts are scored and how parameters are tuned without leakage.
* [Dynasty analysis](concepts/dynasty-analysis.md) - How the application judges a title run against season-specific strength.
* [Historical data](concepts/historical-data.md) - Data sources, coverage, franchise identity and game classification.
* [Elo continuation](concepts/elo-continuation.md) - How the application computes ratings for seasons after the archive ends, and projects a season that has not started.
* [Player data](concepts/player-data.md) - Where the players, their box-score lines and the daily injury reports come from, how they match the stored games, and how a backfill resumes.
* [Player worth and availability](concepts/player-worth.md) - How a player is valued in Elo points from the games his team played without him, and how the injury report becomes a forecast penalty.
* [Box-score worth](concepts/box-score-worth.md) - How a player is valued from his box score with Box Plus/Minus 2.0, VORP and PER, on the same Elo scale.
* [Persistence model](concepts/persistence.md) - Which state is durable, where it lives and which commands keep it.
* [Security model](concepts/security.md) - Threat model of the local-only deployment and the controls in place.
* [Rule periods and bargaining periods](concepts/periods.md) - The period tables that group seasons on the Records page, with what each period changed.
* [Era builder](concepts/era-builder.md) - How the facets on the Records page replay every record table over the seasons of chosen eras, and how the group panel computes one row of records per combination of dimensions.
* [Parity profile](concepts/parity.md) - How the Records page measures, season by season, how far the champion, the finalists, the final four and the playoff field stood above their league, in Elo points and in standard deviations.
* [Question box](concepts/question-box.md) - How the dashboard answers a question from the stored data through a language model with read-only tools, and what it refuses.

# Runbooks

* [Start the application](runbooks/start.md) - First start with Docker Compose.
* [Run an alpha scenario](runbooks/alpha-scenario.md) - Ask the Alpha Simulator how often a title run happens by chance, save and compare scenarios.
* [Upgrade the application](runbooks/upgrade.md) - Rebuild the image and keep all data.
* [Back up and restore](runbooks/backup-restore.md) - Create, list, restore and verify database backups.
* [Verify persistence](runbooks/verify-persistence.md) - Run the automated persistence test.
* [Import and refresh data](runbooks/import-data.md) - Load a CSV file or download the latest game log.
* [Develop and test](runbooks/develop.md) - Build, run and test outside Docker.

# Reference

* [Configuration](reference/configuration.md) - Environment variables and their defaults.
* [HTTP API](reference/api.md) - Pages and JSON endpoints.
* [Database schema](reference/schema.md) - Tables, keys and derived data.
* [Simulation jobs](reference/jobs.md) - Job lifecycle, versions and retry rules.

# Decisions

* [ADR 0001 — Ports at three boundaries](decisions/0001-ports-at-three-boundaries.md) - Ten ports, one store adapter, one composition root.
* [ADR 0002 — Runtime enums for persisted lifecycles](decisions/0002-runtime-enums-over-typestate.md) - Why no typestate.
* [ADR 0003 — Bundle the dataset](decisions/0003-bundled-dataset.md) - Why the game log ships in the image.
* [ADR 0004 — Compute first](decisions/0004-compute-first.md) - Why the application computes every rating itself and keeps the archive's ratings as reference values.
* [ADR 0005 — Question box through OpenRouter](decisions/0005-question-box-via-openrouter.md) - Why the question box uses a hosted model through an OpenAI-compatible API with read-only tools.

# History

* [Update log](log.md) - Chronological history of this bundle.
