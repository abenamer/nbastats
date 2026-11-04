---
type: Architecture Decision
title: YAXEL, an emergence term backtested before it is used
description: Why a rookie or a changed player who plays like a star for a few games is let into a team's rating through his box scores, as a damped sum of surprises against the worth each player entered the season with, and why the rule is backtested over the stored seasons from the command line before it touches a page.
tags: [decision, yaxel, worth, box-score, backtest]
generated: { by: "human:abenamer", at: "2026-11-04T09:00:00Z" }
---

# ADR 0014 — YAXEL, an emergence term backtested before it is used

## Context

The team rating learns from results at K = 20, a half-life of about 25 games for a step change in a team's strength ([ADR 0011](0011-a-worth-what-if-moves-the-team-by-the-difference.md)). A rookie is priced at his draft bracket's mean until his own box worth takes over at about 500 minutes ([Player worth](../concepts/player-worth.md), Rookies), and a player's worth reaches the rating only when he sits. So a rookie who plays like a star from his first game moves his team's rating by a few points in a fortnight, while the sportsbook has repriced the team in a week. The maintainer watched this happen to several rookies in the same autumn, named the problem after one of them, and asked for the model to see what the box scores see.

The papers in the project's research say what to expect ([ADR 0008](0008-track-the-model-against-the-market.md) and the research report): player-built ratings do not beat the market or the 68 percent game-accuracy ceiling (Sill 2010, Hubáček and others 2019, the 2026 valuation paper), and a pre-season roster model helps the first weeks and nothing after (Demsyn-Jones 2019). What a box-score term can buy is the first weeks of a player the entering worth has wrong.

## Decision

* The term is in-season and reads what has already been played. For each player on a team's floor, the surprise is his box worth over a window of days before the game, shrunk toward the worth he entered the season with by his minutes in the window (the two weigh the same at the prior-minutes parameter), less that entering worth. A team's shift is the damped sum of its floor's surprises. The forecast of a game is the Elo forecast with both shifts added. No minutes are forecast: the floor is the players who played in the team's previous game, and the minutes are the ones they played. `domain::yaxel` holds the rule and the scoring; `application::yaxel` builds the samples from the stored games, box scores and the pricing the injury report uses, with the players' entering worths taken from the seasons before only.
* The rule has three parameters, the window in days, the prior minutes and the damping, and none is chosen by hand. A backtest over the stored seasons searches a grid: the seasons in order are split into two folds by turns, each fold's parameters are the grid point with the greatest mean Brier gain over all games of its training seasons, and the test seasons are scored under them, over all games and over the early season (neither team past its thirtieth game). The bar for taking the rule to a page is a gain of two standard errors on the pooled test seasons and on the early season.
* The backtest runs from the command line (`nba-elo-simulator yaxel-backtest`) and prints a report; it writes nothing and changes no rating, forecast or page. The live term, if the backtest earns it, is a later decision with its own record, and would reach the Tonight panel, its market and the tracking the way the worth what-ifs do.

## Consequences

* The maintainer can measure the idea before living with it. The report says, season by season, whether the box scores would have improved the forecasts, by how much, and whether the early season is where the gain sits.
* The rule double-counts what the rating has already absorbed: after a few weeks a star's effect is in the results and in his surprise. The damping is the crude answer, and the backtest chooses it; a decay of the term with games played is the refinement if the gain is there but the chosen damping is small.
* The entering worth of a rookie is his draft bracket, so his surprise is the whole of what his box scores show above it. Six games of box score are noisy; the prior minutes decide how much of six games counts, and the backtest chooses them. College records stay out: the draft slot is the only college signal, and adding another source is a separate decision.
* The backtest uses the players who played in each team's previous game as tonight's floor, so an absence tonight is not known to it; the live term would take the floor from the injury report, as the Tonight panel does. The backtest is therefore a little harder on the rule than the live page would be.

## Alternatives

* A pre-season roster prior (the roster's entering worths blended into the carried-over rating) was scoped and kept for later: it fixes October's roster turnover, not a rookie's emergence. It is YAXEL's second stage if the first earns its place.
* Raising K, or updating the rating from the box-score margin as well as the real one, was rejected: the maintainer asked that the rating stay what it is, and the record judges it.
* Projecting minutes was rejected: minutes at the top of a rotation are the predictable ones and carry the value, and the floor of the previous game with the minutes played needs no forecast.
* Choosing the parameters on the seasons they are scored on was rejected, which is why there are two folds; choosing them by hand was rejected, which is why there is a grid.
