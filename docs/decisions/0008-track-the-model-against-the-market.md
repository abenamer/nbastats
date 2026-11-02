---
type: Architecture Decision
title: Track the model against the market
description: Why each game's model numbers and sportsbook lines are now stored in the last capture before tip-off and scored after the result, amending ADR 0006's "nothing is stored".
tags: [decision, market, tracking, evaluation]
generated: { by: "human:abenamer", at: "2026-11-02T09:00:00Z" }
---

# ADR 0008 — Track the model against the market

## Context

[ADR 0006](0006-market-lines-from-espn-scoreboard.md) shows the night's sportsbook lines beside the model and stores nothing, because the maintainer then wanted a comparison of the coming night only. Comparing night by night, the maintainer now wants to see trends: whether the model's disagreements with the market win or lose over many games, and whether a change to the model helps. ESPN keeps no lines for past games, so they have to be kept as they pass.

## Decision

* A table, `market_snapshots` (migration `0007`), keeps one row per game: the model's numbers and the sportsbook's lines in the last capture before the start, with the model version and the application version. It is keyed by date and team codes, so a re-import of the games keeps it.
* One store port, `MarketStore`, implemented by `PostgresStore`, saves the snapshots (a later capture replaces an earlier one only before the start) and reads a season's.
* A background task captures the current season's night every ten minutes while the ESPN feed is on. (Amended 2026-11-02 by [ADR 0009](0009-market-lines-from-the-odds-api.md): the task now reads once when the night's date comes and once before each tip-off, from The Odds API when a key is set.) `MARKET_TRACKING_ENABLED=false` turns it off. A page view starts no capture, so an unauthenticated request causes no model computation or write beyond what the page already does.
* `domain::tracking` scores a snapshot against its result: the Brier score and the log loss of the model and of the market, the lean on the moneyline and the side against the spread at the sportsbook's prices. The record groups them overall, per model version and per month.

## Consequences

* The record starts on the day tracking is first on. Earlier games have no lines to score.
* A model change shows as a new version with its own row, so its effect can be read from the record once it has games.
* The application makes one ESPN request every ten minutes while it runs, besides the page's own reads. (Since ADR 0009: a read per tip-off time instead.)
* The market source stays swappable: a paid odds service would be another `MarketSource` adapter, and the snapshots name their sportsbook and source. (Done 2026-11-02 by [ADR 0009](0009-market-lines-from-the-odds-api.md), which also limits the captures to the night's date.)

## Alternatives

* Keeping the lines only on the page (ADR 0006) was the earlier choice; it cannot show trends.
* Storing every capture rather than the last before the start was rejected: the closing line is the one the market settles on, and the earlier captures add rows without adding to the scoring.
* Buying historical odds now was rejected: the free feed covers the record from today, and a paid service can be added later behind the same port.
