---
type: Concept
title: Market tracking
description: How each game's model numbers and sportsbook lines are saved before tip-off and scored after the result, per model version, to see whether the model beats the market.
tags: [market, tracking, odds, evaluation]
generated: { by: "human:abenamer", at: "2026-11-02T09:00:00Z" }
sources:
  - id: tracking
    resource: /src/domain/tracking.rs
    title: Snapshots and their scores
  - id: app
    resource: /src/application/tracking.rs
    title: Capture and track record
  - id: migration
    resource: /migrations/0007_market_snapshots.sql
    title: Snapshot table
---

# Purpose

The Tonight panel sets the model's numbers beside a sportsbook's for the coming night. Market tracking keeps them, so that once the games are played the model can be judged against the market: on its chances, and on the bets its disagreements would have made. Each saved game carries the model's version, so a change to the model starts a new record instead of blending into the old one.

# Capture

Every ten minutes, while the application runs with the ESPN feed on (`GAME_REPORTS_ENABLED`), a background task reads the Tonight panel of the current season and its sportsbook lines ([Market lines](/decisions/0006-market-lines-from-espn-scoreboard.md)). `MARKET_TRACKING_ENABLED` turns it on by default; any value other than `1`, `true` or `yes` turns it off. Viewing the page starts no capture. For each game the sportsbook prices that has not started, it saves a snapshot (`domain::tracking::Snapshot`):

* the model's home chance at full strength and on the night, its expected home margin on the night, the full-strength and stored ratings, the penalties of the night and the home bonus;
* the sportsbook's spread for the home team with the price of each side, both moneylines, the total, the home chance its moneylines imply with the margin taken out, and that margin;
* the sportsbook, the source, the game's start time, the time the lines were read, the model version and the application version.

The model version joins the versions of the parts that make the night's numbers: the full-strength rating, the pooling of a player's worth, the box calibration, the box-score worth, the conversion of a rating gap to points, the status curves, the spell curve and the draft priors. A snapshot is keyed by date and team codes, as the game reports are, so a re-import of the games keeps it. A later capture replaces an earlier one only while the game has not started: before the capture's own start time when it has one (a moved tip), else the stored one. A capture without a start time keeps the stored start. The last capture before the start stands for the game. That line is within about fifteen minutes of the closing line: the ten minutes between captures and the five the lines stay in the cache. ESPN keeps no lines for past games, so the record starts on the day tracking is first on.

# Scoring

Once a game's result is stored, its snapshot is scored (`domain::tracking::score`):

* **Brier score**: the squared difference between the home chance and the result (1 for a home win, 0 for a loss), for the model and for the market. Lower is better.
* **Log loss**: minus the logarithm of the chance each gave to what happened. Lower is better, and it punishes a confident miss harder than the Brier score.
* **Lean**: the side whose chance the model puts above the market's. Its result is a one-unit bet on that side at the sportsbook's moneyline: a win at +114 returns 1.14 units, a win at -135 returns 100/135, and a loss costs one unit.
* **Against the spread**: the side the model's expected margin takes against the sportsbook's line, bet at that side's price (-110 when the price is missing). A push returns nothing.

A tie scores nothing. A snapshot whose game is not yet played is pending; one that matches no stored game (a game moved to another date), or whose stored score is a tie, is counted apart as unmatched. The three counts add up to the snapshots. The track record (`application::tracking::record`, `GET /api/seasons/{season}/market/track`) sums the scored games overall, per model version and per month. The Brier and log loss means of the model and the market are over the same games: those the sportsbook priced. A return above zero per bet beats the sportsbook's margin. The Season Forecast page shows the record in its **Model vs market** panel, with the scored games, latest first.

# Limits

* A few dozen games say little: the return of a bet has a spread of about one unit, so a season of a thousand games puts the error of a mean return at about three cents a unit. The Brier difference settles sooner.
* The lines are one sportsbook's, read up to about fifteen minutes before the start; a line that moved in the last minutes is not seen.
* Two rare cases keep an older line: a game pushed later in the night whose later captures carry no start time stops at the first start, and a start corrected to earlier than a stored capture leaves that capture in place.
* The model's numbers are those of the moment of the capture. A model change made since does not rescore old games; the version on each row says which model made it.
* A paid odds service could replace the ESPN feed behind the same `MarketSource` port; the snapshots name their sportsbook and source, so the record keeps both.
