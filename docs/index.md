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
* [Return from absence](concepts/return-from-absence.md) - A report of what regular players deliver after a run of missed games against before, by their BPM and the run's length, with a matched control; it changes no forecast.
* [Persistence model](concepts/persistence.md) - Which state is durable, where it lives and which commands keep it.
* [Security model](concepts/security.md) - Threat model of the local-only deployment and the controls in place.
* [Rule periods and bargaining periods](concepts/periods.md) - The period tables that group seasons on the Records page, with what each period changed.
* [Era builder](concepts/era-builder.md) - How the facets on the Records page replay every record table over the seasons of chosen eras, and how the group panel computes one row of records per combination of dimensions.
* [Parity profile](concepts/parity.md) - How the Records page measures, season by season, how far the champion, the finalists, the final four and the playoff field stood above their league, in Elo points and in standard deviations.
* [Question box](concepts/question-box.md) - How the dashboard answers a question from the stored data through a language model with read-only tools, and what it refuses.
* [Market tracking](concepts/market-tracking.md) - How each game's model numbers and sportsbook lines are saved before tip-off and scored after the result, per model version.
* [YAXEL, the emergence term](concepts/yaxel.md) - How a player's recent box scores could move his team's rating before the results do, and how the rule is backtested before it is used.

# Runbooks

* [Start the application](runbooks/start.md) - First start with Docker Compose.
* [Reach the dashboard over Tailscale](runbooks/tailscale.md) - Open the dashboard from your other devices through Tailscale Serve, over HTTPS, with no port open to the internet.
* [Run an alpha scenario](runbooks/alpha-scenario.md) - Ask the Alpha Simulator how often a title run happens by chance, save and compare scenarios.
* [Upgrade the application](runbooks/upgrade.md) - Rebuild the image and keep all data.
* [Back up and restore](runbooks/backup-restore.md) - Create, list, restore and verify database backups.
* [Verify persistence](runbooks/verify-persistence.md) - Run the automated persistence test.
* [Import and refresh data](runbooks/import-data.md) - Load a CSV file or download the latest game log.
* [Run the YAXEL backtest](runbooks/yaxel-backtest.md) - Score the emergence term over the stored seasons from the command line and read the report.
* [Backfill the market record](runbooks/market-backfill.md) - Fill the Model vs Market record from The Odds API's history for the nights the live capture did not see, and what it costs.
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
* [ADR 0006 — Market lines from ESPN's scoreboard](decisions/0006-market-lines-from-espn-scoreboard.md) - Why the Tonight panel reads the night's sportsbook lines from ESPN's free feed, not stored, instead of a paid odds service.
* [ADR 0007 — Full-strength rating from a replayed season](decisions/0007-full-strength-rating.md) - Why tonight's absences are priced off a rating whose played games are replayed with each side's absences, instead of off the stored Elo.
* [ADR 0008 — Track the model against the market](decisions/0008-track-the-model-against-the-market.md) - Why the model's numbers and the sportsbook's lines are stored before tip-off and scored after, amending ADR 0006.
* [ADR 0009 — Market lines from The Odds API, with ESPN as the fallback](decisions/0009-market-lines-from-the-odds-api.md) - Why one chosen sportsbook's lines come from The Odds API when a key is set, ESPN's scoreboard stays the fallback, and captures run only on the night's date.
* [ADR 0010 — Flag a bet by its expected return at the posted price](decisions/0010-flag-a-bet-by-its-expected-return.md) - Why the Tonight panel marks a bet when the model's chance at the sportsbook's price returns a chosen share of the stake, with spreads on a measured scatter, and the record scores the flagged bets apart.
* [ADR 0011 — A worth what-if moves the team by the difference](decisions/0011-a-worth-what-if-moves-the-team-by-the-difference.md) - Why a reader's what-if on a player's worth is a rating shift of the difference from the model's estimate plus his absences at the what-if, in the forecast, the Tonight panel and the injury card, without changing the model.
* [ADR 0012 — Cup games from the source's stage labels](decisions/0012-cup-games-from-the-stage-labels.md) - Why the season engine tells the Cup games by the stage labels the results source puts on each game, with the curated dates kept for the seasons before the labels and as the fallback, and why a stored Cup season without labels is fetched whole once.
* [ADR 0013 — Call the model against the market by the paired Brier gap](decisions/0013-call-the-model-against-the-market-by-the-paired-brier-gap.md) - Why the Model vs Market page calls the more accurate side by the mean game-by-game Brier gap in standard errors, two as the bar, and tells how many games it takes to reach it.
* [ADR 0014 — YAXEL, an emergence term backtested before it is used](decisions/0014-yaxel-an-emergence-term-backtested-before-it-is-used.md) - Why a player's recent box scores, against the worth he entered the season with, could move his team's rating, and why the rule is backtested from the command line before it touches a page.
* [ADR 0015 — Mint and rotate the question box's key from a management key](decisions/0015-mint-and-rotate-the-question-box-key-from-a-management-key.md) - Why the application mints its own OpenRouter completion key from a management key, replaces it on a schedule and when the service refuses it, deletes the keys it minted before, and keeps no secret of its own.
* [ADR 0016 — Track the model's title chances against the sportsbook's futures](decisions/0016-track-the-title-chances-against-the-sportsbooks-futures.md) - Why the application saves, once a day through the regular season, the sportsbook's outright prices on the title beside the model's title chance of the same day, how the prices become chances, and how the days are scored against the champion.
* [ADR 0018 — Backfill the market record from the sportsbook's history](decisions/0018-backfill-the-market-record-from-the-sportsbooks-history.md) - Why the nights before the tracking was on, and the seasons before, are filled from The Odds API's stored snapshots of the lines read before the tip-off, beside the stored pre-game Elo alone under its own model version, and how the plan's credits are spent.
* [ADR 0017 — A What if page runs counterfactuals through the season engine](decisions/0017-a-what-if-page-runs-counterfactuals-through-the-season-engine.md) - Why a real season can be run again with a team's rating set by hand, a team barred from the postseason, another era's playoff format, players out or a player's worth changed, as a second run of the season engine beside the plain one, and what the engine takes as parameters for it.

# History

* [Update log](log.md) - Chronological history of this bundle.
