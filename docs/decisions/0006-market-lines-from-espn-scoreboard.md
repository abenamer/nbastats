---
type: Architecture Decision
title: Market lines from ESPN's scoreboard
description: Why the Tonight panel reads the night's sportsbook lines from ESPN's free scoreboard feed, shown beside the model and not stored, instead of a paid odds service.
tags: [decision, market, espn, odds]
generated: { by: "human:abenamer", at: "2026-11-01T09:00:00Z" }
---

# ADR 0006 — Market lines from ESPN's scoreboard

## Context

The maintainer compares the Tonight panel's spreads and moneylines with a sportsbook's to see whether the model and the market agree. Typing the lines in by hand for each game is slow. The BALLDONTLIE plan in use (ALL-STAR) has no betting odds: its odds endpoint is in the GOAT tier. A dedicated odds service has a free tier too small for history and costs money beyond it. The maintainer wants a comparison of the coming night only, not an archive or a backtest.

ESPN's public scoreboard feed, which the application already reads for the game report links, carries one sportsbook's lines for each game not yet played: the spread with the price of each side, both moneylines and the total, at the open and now. At the time of writing the sportsbook is DraftKings. Past dates come back without lines.

## Decision

* One port, `MarketSource`, with one method: the lines for a date. The concrete adapter is `EspnScoreboard` (renamed from `EspnReportSource` with this decision), which already reads the same feed for `ReportSource`; one fetch routine serves either port, each making its own request. (Amended 2026-11-02 by [ADR 0009](0009-market-lines-from-the-odds-api.md): with an Odds API key, `OddsApi` is the first source and this feed the fallback.)
* `application::market::tonight_market` reads the lines for the Tonight panel's night when asked, one read at a time, and keeps them five minutes (a failed read one minute), so reloads and concurrent requests ask ESPN at most once per date in that time. It matches the lines to the night's games by team, the way the game reports match. Nothing is stored. (Amended 2026-11-02 by [ADR 0008](0008-track-the-model-against-the-market.md): the last capture before each game's start is now stored and scored.)
* `domain::market` reads a moneyline as the probability it implies and takes the bookmaker's margin out of a game's two moneylines by scaling them to add up to 1. The panel shows the sportsbook's spread, moneyline and that chance beside the model's, with the difference.
* The lines use the existing switch: `GAME_REPORTS_ENABLED=false` turns them off with the reports. A source that is off or fails answers with a note, and the panel still shows the model.

## Consequences

* The comparison costs nothing and needs no key.
* The feed is public but not documented. ESPN can change its shape or drop the lines; the parser reads two shapes seen so far, and a change shows as a note, not an error.
* The lines are one sportsbook's, not a consensus, and they move until tip-off. The panel names the sportsbook and the time the lines were read.
* With nothing stored, the application cannot score the model against the market over many games. That needs the lines kept each night or loaded from an archive, which the maintainer did not ask for.

## Alternatives

* The BALLDONTLIE GOAT tier was rejected for now: a higher plan for one comparison.
* A dedicated odds service (The Odds API and others) was rejected: its free tier serves current lines but makes history expensive, and it adds a key and a provider.
* Lines typed by hand on the panel were rejected: slow, and the feed already has them.
* An archive of past lines imported as CSV was rejected: the maintainer wants tonight's comparison only.
