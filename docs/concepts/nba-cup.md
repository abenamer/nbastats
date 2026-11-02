---
type: Model
title: NBA Cup forecast
description: How the season forecast simulates the NBA Cup from the schedule, with the groups derived from the group nights, the Cup's tiebreaks and a projected knockout bracket.
tags: [nba-cup, season, forecast, engine-c]
generated: { by: "human:abenamer", at: "2026-10-24T06:00:00Z" }
sources:
  - id: code
    resource: /src/domain/cup.rs
    title: Cup module
  - id: engine
    resource: /src/domain/playoffs/season.rs
    title: Season engine
---

# Purpose

The NBA Cup is the in-season tournament the league has played since 2023-24. The Season Forecast page gives every team's chance to reach the knockout, the final and win the Cup, and draws the most likely knockout bracket. The forecast needs no curated group tables: the groups follow from the schedule.

# Data

The forecast needs to know which games are Cup games and of which round. From the 2025-26 season on the results feed says so. The results source labels each Cup game with its stage (`East Group A` to `West Group C`, `East Quarterfinal`, `West Quarterfinal`, `East Semifinal`, `West Semifinal`, `Championship`), the application stores the label with the game (`Game::cup_stage`, the `cup_stage` column of `games`; a stored label is kept when a later fetch brings none, so only removing the season clears one), and the season engine reads it: a game labelled with a group stage is a group game, one labelled with a conference quarterfinal or semifinal a knockout game, the `Championship` the final. For a season with no labelled game the engine falls back to a curated record of dates in `domain::cup`, the group nights, the quarterfinal dates, the semifinal date and the final's date: a regular-season game on a group night is a group game, a game on a knockout date a knockout game. The dates of 2023-24, 2024-25 and 2025-26 are in the table, from the league's announcements. A season with both uses its labels, unless the labelled group games do not form the groups while the dated ones do; then the dates stand in. A Cup season with neither has no Cup forecast, and the page says why and names the two remedies: sync the results, or add the dates. See [ADR 0012](../decisions/0012-cup-games-from-the-stage-labels.md).

A season stored before the labels were read has none. The results sync notices this: for a season from 2025-26 on whose synced games all lack a stage, one sync fetches the whole season instead of the days since the last played game, after which the daily window carries each Cup game's label on its day (see [Import and refresh data](../runbooks/import-data.md)).

The groups are derived, not stored. Each team plays its four group-mates once, so the group games connect the league into six groups of five, three per conference. The derivation checks this shape: every team with exactly four games, no pair twice, no game across conferences, six groups of five. On 2026-10-24 the groups derived from the imported schedule were checked by hand against the league's published groups for 2023-24 and 2024-25, and they match. When the shape does not hold (a schedule not yet published, a game moved off its night), there is no Cup forecast and the page says why. The group letters on the page are the application's, by the lowest team identifier in the group, not the league's.

# Procedure for one simulated season

1. The group games are regular-season games, so the season forecast already simulates them. For a group game yet to come, the engine also draws a margin. The margin is a draw from the normal distribution around the Elo expectation (one point per 28 Elo points of edge, home bonus included) with a standard deviation of 12 points, conditional on the side the game went to, and at least one point. A favourite that lost therefore loses by a small margin more often than by a large one. A played group game keeps its result and margin.
2. Each group's table: wins first. Tied teams follow the league's procedure: rank them by their record against each other, then by point differential over the group games, then by a random key; take the top team out and restart with the teams that remain. The league also counts points scored and the previous season's record before its drawing; the simulation has neither.
3. The knockout seeds of each conference: the three group winners by wins, then differential, then key. The best runner-up by the same order is the wild card, seed 4.
4. The quarterfinals: seed 1 hosts seed 4 and seed 2 hosts seed 3, decided by Elo with the regular-season home-court bonus. The semifinals and the final are on a neutral floor. A knockout game played on or before the cutoff keeps its result, the final included. A knockout game the schedule already holds is simulated as a regular-season game, and the Cup keeps that result. A simulated knockout whose pairing differs from a played or scheduled game is decided by Elo.
5. A knockout game the schedule does not hold yet feeds neither the standings nor the ratings of the simulated season. The ratings used are the ratings the simulation holds at the end of its regular season: the cutoff date's ratings, or the updated ones when a K-factor is set.

# Outputs

* For every team: the chance to reach the knockout, the semifinals, the final, and to win the Cup, over the simulated seasons in which the knockout could be formed.
* Each group's table from the group games played on or before the cutoff.
* The projected Cup bracket. For each conference and seed in turn, the slot goes to the team that held the seed most often among the teams not yet placed. A seed whose holders are all placed goes to the unplaced team that reached the knockout most often. The chances through that bracket are exact at the cutoff date's ratings, with played knockout games decided. The counts say how often this exact bracket, these eight teams in any seeding, and the most frequent bracket of all occurred among the first 200,000 simulated seasons. As with the [projected playoff bracket](season-engine.md), it is one bracket of very many.

# Limits

* Knockout games the schedule does not hold yet are not in the simulated standings. The league publishes 80 games per team and adds the two Cup-week games in December, once the knockout field is known. Until then a stored schedule holds 80 games, and the forecast simulates those. The two games each team plays in Cup week are a small share of a season. Their effect on the seeding is below the noise of the forecast. The question box says so when a schedule is short (`domain::cup::games_to_be_scheduled`).
* Margins are simulated for the tiebreaks only, from a simple normal model. The forecast never uses them elsewhere.
* The labels, the curated dates and the derived groups are only as good as the stored schedule. A season whose schedule is a projection, not the league's, derives groups from that projection, and the shape check usually fails.
* The season model version is `elo-season-1.2` from this change. A result stored under an earlier version has no Cup forecast.
