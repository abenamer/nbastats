---
type: Architecture Decision
title: Call the model against the market by the paired Brier gap
description: Why the Model vs Market page says which of the model and the sportsbook has been the more accurate by the mean of the game-by-game Brier gap in standard errors, with two errors as the bar, and tells how many games it takes to reach it.
tags: [decision, market, tracking, evaluation]
generated: { by: "human:abenamer", at: "2026-11-03T09:00:00Z" }
---

# ADR 0013 — Call the model against the market by the paired Brier gap

## Context

Market tracking ([ADR 0008](0008-track-the-model-against-the-market.md)) saves each game's model numbers and sportsbook lines before tip-off and scores both against the result, and the Season Forecast page shows the record in a panel of mean scores and betting records. The maintainer asked for one page that lists every game's odds, the result and the model's number, and simply says which is more accurate. Two mean Brier scores side by side do not say that: a lead of a few thousandths over forty games is luck as often as not, and the panel left the reader to judge.

## Decision

* The comparison is paired. For each game both priced, the gap is the sportsbook's Brier score less the model's, so a positive gap favours the model. The verdict rests on the mean gap over the games, its standard error (the sample standard deviation of the gaps over the square root of their count) and the mean in standard errors (`domain::tracking::compare`). The side with the lower mean score leads; the lead is called when the mean gap reaches two standard errors (`DECISIVE_Z`), about one chance in twenty that luck alone did it, and from thirty games both priced on (`MIN_GAMES_TO_CALL`), since the mean of the gaps is near enough normal from a few dozen games and two games with the same gap would otherwise call it; under either bar the page says the two are "not yet apart" or that the games are too few.
* The comparison also counts the games each side was the closer on, and says how many games it takes to reach two standard errors at the mean gap and spread seen so far: the square of twice the standard deviation over the mean gap, rounded up and at least the thirty-game minimum, so a lead is decided exactly when the games reach the figure. One game, or games with no spread in their gaps, give a leader without an error, and the page says so instead of calling it. A mean gap too small to tell apart from zero is level, with no leader; a gap that would take more than ten seasons of games (`MAX_GAMES_TO_TELL`, 12,300) to call reports no games to tell.
* The comparison is made overall, per model version and per month, and travels with the track record (`comparison` on `GET /api/seasons/{season}/market/track` and on each group). The record now takes a `limit` on the games it lists, 300 by default and 2,000 at most, so the page can list a whole season.
* A new page, **Model vs Market** (`/market`), puts the verdict in a sentence and five cards, the summary table under it, and every scored game of the season with the model's chance, the sportsbook's, which was the closer, both spreads, the final margin, the moneylines, the lean and the flagged bets. The Season Forecast page keeps its panel and links to the page.

## Consequences

* A reader gets a plain answer and the reason for it in the same breath: who leads, by how much, and whether that is more than luck. The "games to tell" figure sets expectations for the season: a gap of a few thousandths of Brier against a spread of about 0.3 per game takes hundreds of games to call.
* The Brier gap is the one score called; log loss is shown beside it and not called, so the page gives one verdict rather than two that may disagree.
* The test treats the games as independent and the gaps as roughly normal in the mean, which the central limit theorem gives from a few dozen games on. The games of one night share the injury report and the ratings, so the standard error is a little understated; the bar of two errors leaves room for that.
* Betting records stay as records: a return per bet is a different question from accuracy, and its error is far larger, as the Market tracking concept's limits say.

## Alternatives

* Calling the lead from the mean Brier scores alone, or from the count of games each was closer on (a sign test), was rejected: the first has no error, and the second throws away how far off each side was. The count is shown because it is easy to read, not because it decides.
* A Bayesian comparison or a bootstrap was not taken: the paired z is the standard test for two forecasters on the same games, has no tunable parts, and is explained in one sentence on the page.
* Calling the lead by log loss was rejected as the headline: a single confident miss moves it far more than a Brier score, and the sportsbook's margin-free chance and the model's chance are compared on the Brier scale throughout the application.
