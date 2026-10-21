---
type: Model
title: Season forecast engine
description: Engine C simulates the rest of a season from a cutoff date and gives every team's probability for each stage.
tags: [engine-c, season, play-in, forecast]
generated: { by: "human:abenamer", at: "2026-10-21T17:30:00Z" }
sources:
  - id: code
    resource: /src/playoffs/season.rs
    title: Season engine source
---

# Purpose

The Season Forecast page shows, for every team, the probability to:

* finish in the top six of its conference,
* play in the play-in tournament,
* reach the playoffs (first round),
* reach the second round,
* reach the conference finals,
* reach the NBA Finals,
* win the championship.

# Procedure for one simulated season

1. Take each team's Elo after its last game on or before the cutoff date. Before the first game, take the preseason rating from the data.
2. Count the standings from the games played on or before the cutoff.
3. Simulate every later regular-season game from Elo. Update ratings with the regular-season K-factor (default 20).
4. Order each conference with the season's tiebreak rules. Ties use the group head-to-head record, then point differential, then a seeded random key.
5. Resolve the play-in with a state machine:
   * 2020-21 and later: 7 hosts 8; 9 hosts 10; the loser of the first game hosts the winner of the second game.
   * 2019-20: the ninth team plays the eighth team when it is within four games. The ninth team must win twice.
   * Before 2019-20: the top eight qualify.
   A play-in game that was played on or before the cutoff keeps its real result.
6. Build the bracket and play it with the playoff model. Playoff ratings are fixed unless you set a playoff K-factor.

# Information cutoff

Only games on or before the cutoff date inform the forecast. A cutoff inside the playoffs is moved back to the day before the first playoff game. The integration test `forecasts_never_use_future_information` checks that a forecast does not change when later games are removed from the input.
