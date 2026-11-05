---
type: Concept
title: YAXEL, the emergence term
description: How a player's recent box scores, set against the worth he entered the season with, could move his team's rating before the results do, and how the rule is backtested over the stored seasons before it is used.
tags: [yaxel, worth, box-score, backtest, rating]
generated: { by: "human:abenamer", at: "2026-11-04T09:00:00Z" }
sources:
  - id: domain
    resource: /src/domain/yaxel.rs
    title: The rule and its scoring
  - id: app
    resource: /src/application/yaxel.rs
    title: The samples and the report
---

# Purpose

The team rating learns from results, at a pace of about 25 games for half of a step change. A rookie, or a player who changed, can play like a star for a week while the rating still carries last season's view of his team. YAXEL is the term that lets his box scores in: a shift to his team's rating from what his recent lines say beyond the worth he entered the season with. It is named for the rookie whose first weeks the market priced and the rating did not. See [ADR 0014](/decisions/0014-yaxel-an-emergence-term-backtested-before-it-is-used.md).

# The rule

For a game, each team's floor is the players who played in the team's previous game. For each of them:

* The entering worth is the worth the model priced him at coming into the season: from the games his team played without him and his box scores in the seasons before, or for a rookie his draft bracket's mean scaled by the box calibration ([Player worth](/concepts/player-worth.md)). A player with no estimate, a rookie whose bracket prices as nothing among them, enters at 0, the replacement level. Nothing from the season itself.
* The recent box worth is his box worth over the window of days before the game: the box-score table built from the league's games in that window alone, his lines pooled by minutes, scaled by the box calibration to the worth scale, as the what-if window is ([Season forecast engine](/concepts/season-engine.md)).
* The surprise is the recent box worth shrunk toward the entering worth by his minutes in the window, less the entering worth: `(recent − entering) × minutes / (minutes + prior minutes)`. At the prior minutes the two weigh the same; with no minutes there is no surprise.

The team's shift is the damping times the sum of its floor's surprises. The forecast of the game is the Elo forecast with each team's shift added to its pre-game rating, the home bonus as the season's fitted one.

Three parameters: the window in days, the prior minutes and the damping. None is set by hand; the backtest chooses them.

# The backtest

`nba-elo-simulator yaxel-backtest --from 2010 --to 2026` samples every season in the range that has at least 100 played regular-season games, box scores with BPM and an earlier season to price the players from (a season without is listed as skipped, with the reason). For each game it keeps the stored pre-game ratings, the result, each team's game number and, per window of the grid, the two floors. Then `domain::yaxel::backtest` scores the rule:

* The grid is windows of 7, 14 and 28 days, prior minutes of 100, 250 and 500, dampings of 0.25, 0.5, 0.75 and 1.
* The seasons in order are split into two folds by turns. Each fold's parameters are the grid point with the greatest mean Brier gain, base Brier less YAXEL Brier over all games of its training seasons; the fold's test seasons are scored under them. So no season is scored under parameters chosen on it.
* The scores are the mean Brier score and log loss of each forecast, the mean gain, its standard error and the gain in standard errors, over all games and over the early season: games in which neither team has played more than thirty (`--through` sets the cut).
* The report prints each fold's choice and test score, every test season under its fold's parameters, and the test seasons pooled; `--json <path>` writes it whole.

The bar for taking the rule to a page is a gain of two standard errors on the pooled test seasons and on the early season. The backtest writes nothing and changes no rating or page. The deciding run over the stored history (17 seasons, 2026-11-05) did not clear the bar, so the live term is off; [ADR 0014](/decisions/0014-yaxel-an-emergence-term-backtested-before-it-is-used.md) records the run.

# Limits

* The backtest's floor is the previous game's players, so an absence tonight is not known to it; the live term would read the injury report. The backtest is a little harder on the rule than the page would be.
* After a few weeks a player's effect is in the results and in his surprise alike; the damping is the only guard against counting it twice, and a decay with games played is the refinement if the gain is there.
* A rookie's entering worth is his draft bracket, so a rookie's surprise is everything his box scores show above it, with six games' worth of noise. The prior minutes set how much of that counts.
* The window table is built from a few days of games, so its team adjustment is rough; the window and the calibration carry that.
* The gain in standard errors treats the games as independent. Games of one date share a window table and a team's games follow one another, so the error is a little understated and the bar of two leaves room for that.
* The numbers in the report depend on the store they are run against; the ones in this bundle are the runs recorded in the decision record, the deciding one on the maintainer's store.
