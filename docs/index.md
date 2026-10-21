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
* [Alpha lottery engine](concepts/alpha-engine.md) - Engine A: the alpha-weighted championship lottery and its exact check.
* [Playoff simulator](concepts/playoff-engine.md) - Engine B: bracket reconstruction, exact probabilities and Monte Carlo.
* [Season forecast engine](concepts/season-engine.md) - Engine C: full-season simulation with play-in and playoffs.
* [Backtesting method](concepts/backtesting.md) - How forecasts are scored and how parameters are tuned without leakage.
* [Dynasty analysis](concepts/dynasty-analysis.md) - How the application judges a title run against season-specific strength.
* [Historical data](concepts/historical-data.md) - Data sources, coverage, franchise identity and game classification.
* [Persistence model](concepts/persistence.md) - Which state is durable, where it lives and which commands keep it.

# Runbooks

* [Start the application](runbooks/start.md) - First start with Docker Compose.
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

* [ADR 0001 — Ports at three boundaries](decisions/0001-ports-at-three-boundaries.md) - Six ports, one store adapter, one composition root.
* [ADR 0002 — Runtime enums for persisted lifecycles](decisions/0002-runtime-enums-over-typestate.md) - Why no typestate.
* [ADR 0003 — Bundle the dataset](decisions/0003-bundled-dataset.md) - Why the game log ships in the image.

# History

* [Update log](log.md) - Chronological history of this bundle.
