---
type: Architecture Decision
title: Track the model's title chances against the sportsbook's futures
description: Why the application saves, once a day through the regular season, the sportsbook's outright prices on the title beside the model's title chance of the same day, how the prices become chances, and how the days are scored against the champion.
tags: [decision, market, tracking, futures, evaluation]
generated: { by: "human:abenamer", at: "2026-11-05T12:00:00Z" }
---

# ADR 0016 — Track the model's title chances against the sportsbook's futures

## Context

Market tracking ([ADR 0008](0008-track-the-model-against-the-market.md)) judges the model game by game, on the chances it gives the night's games against the sportsbook's lines. The season forecast's headline is a different number: each team's chance of the title. The maintainer asked for the model to be judged on that too, against the sportsbook's championship futures, the outright prices on every team. A futures price is one sportsbook's view of a whole season, read on one day; the only fair comparison is the model's forecast of the same day, and the only verdict comes at the end, when the champion is known.

## Decision

* Once a day while the regular season runs (the day's first look of the market tracking task, which looks every ten minutes), the application reads the sportsbook's outright prices on the title through The Odds API, the `outrights` market of the championship sport, one credit a day. The read goes through the `FuturesSource` port, implemented by the Odds API adapter; ESPN's scoreboard carries no futures, so the capture needs `ODDS_API_KEY`. A read or forecast that fails is tried again after an hour; an answer the application cannot keep (no outright market, no team of the season named, or not an outright listing) waits for the next day, so a configuration that cannot work costs one credit a day.
* The same day the application forecasts the season from that day with the injury report, from the full-strength ratings, at 2,000 simulated seasons with a fixed seed, as the question box's comparison runs it, so the model's title chance is of the day and not of a forecast somebody happened to run. Both chances are saved per team and day (`futures_snapshots`, one row per team per day; a later capture of the day replaces an earlier one), with the sportsbook's price, the field's margin, the model version and the application version.
* The sportsbook's chance is its price with the field's margin taken out by the proportional rule: every implied chance scaled by the same factor so the field adds up to one (`domain::market::fair_field`), the same rule the two moneylines of a game take. The margin is kept beside the chances. The rule leaves the long shots a little high, since a bookmaker's margin sits mostly on them; it is kept for being the one rule used throughout, and the margin tells a reader how much was taken out.
* The playoffs are not captured. The season engine forecasts from the day before the first playoff game at the latest, so a title chance during the playoffs would not be the model's chance on the day. The record is of the regular season's days.
* Once the postseason is complete, each saved day is scored against the champion (`domain::futures::score_days`): the squared error summed over the teams, the Brier score of a field, and minus the log of the chance given to the champion, for the model and for the sportsbook; the days say which was the closer and how often. A day whose field lacks the champion is skipped. The Model vs Market page shows the latest day's field, the model beside the sportsbook with the gap in points, the days saved, and the record once the season is decided (`GET /api/seasons/{season}/futures`).

## Consequences

* A season gives one verdict from about 170 days, and the days are far from independent: the same sportsbook, the same forecast with a day's news added. The count of days the model was closer is a description, not a test; no standard error is put on it, unlike the game-by-game comparison ([ADR 0013](0013-call-the-model-against-the-market-by-the-paired-brier-gap.md)). The judgement firms up over seasons.
* The daily forecast costs a few seconds of CPU a day and one credit of The Odds API's plan; the market tracking's night costs about twenty, so the futures add a few percent.
* A model change starts a new version on the rows, as it does for the game snapshots, and the page names the version of the field it shows.
* A team the sportsbook names in a way the store does not know (a renamed franchise, or a line for "any other team") is left out of the field and logged; the field's margin is taken out over the teams kept.

## Alternatives

* Reading the futures from the forecast the maintainer last ran on the Season page was rejected: it may be days old and run with other settings, and the comparison would not be of the same day.
* Capturing through the playoffs with the playoff engine's exact chances was rejected for now: those chances are from the start of the playoffs, not conditioned on the series played, so a team already out would keep its chance. A conditioned playoff chance can be added later and would extend the record, not change it.
* Taking the margin out with a method that bends the long shots down (a power rule or Shin's method) was rejected for now: one rule throughout is simpler to read, and the margin is shown; the rule can change with a model version.
* A standard error on the day count, as the paired comparison has, was rejected: the days are not independent, and an error that pretends they are would overstate the verdict.
