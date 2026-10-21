---
type: Model
title: Playoff simulator
description: Engine B reconstructs the historical bracket and computes championship probabilities exactly and by Monte Carlo.
tags: [engine-b, playoffs, bracket, monte-carlo]
generated: { by: "human:abenamer", at: "2026-10-21T17:30:00Z" }
sources:
  - id: bracket
    resource: /src/playoffs/bracket.rs
    title: Bracket model
  - id: reconstruct
    resource: /src/playoffs/reconstruct.rs
    title: Bracket reconstruction
  - id: seeding
    resource: /src/playoffs/seeding.rs
    title: Seeding rules
---

# Bracket

The bracket has 16 slots. Slots 0 to 7 are the East. Slots 8 to 15 are the West. The positions in a conference hold seeds 1, 8, 4, 5, 2, 7, 3, 6. Round 1 pairs adjacent slots. Each later round pairs the winners of adjacent pairs. The NBA has used this fixed bracket since 1984.

# Reconstruction from game records

The application does not store seeds. It rebuilds the bracket from the playoff games:

1. Group playoff games into series by team pair.
2. Order series by first game. A series is in round r when a participant's previous series was in round r − 1.
3. Compute the regular-season standings and apply the seeding rules of the season (see below).
4. Place the series with the best-ranked team in the 1-8 position. Use the second-round data to find its partner series (4-5). Rank the two other series as 2-7 and 3-6.
5. Record the game-1 host of each series. Hypothetical matchups give home court to the better record, then the better seed.

Bracket pairings always come from the data. A note is shown when the rule-based seed number differs from the bracket position.

# Seeding rules by era

| Seasons | Rule |
|---------|------|
| through 2003-04 | Two division winners take seeds 1 and 2. |
| 2004-05 and 2005-06 | Three division winners take seeds 1 to 3. |
| 2006-07 to 2014-15 | Division winners get a top-four seed. |
| 2015-16 and later | Record only. |

Ties follow the NBA procedure: division winners first, then the head-to-head record inside the tied group, then division record, then conference record, then point differential. After the top team leaves a tie, the procedure restarts for the teams that remain.

# Known limitation

The deeper NBA tiebreak criteria (record against playoff teams, net rating) are not implemented. In three seasons (2002-03, 2018-19, 2021-22) the rule-based seed number of a few teams differs from the bracket position. The bracket pairings always come from the real games, so the simulation is not affected. The Playoff Simulator page shows a note for each such team.

# Strength models

| Model | Game probability |
|-------|------------------|
| Elo | Elo formula with home bonus and scale. |
| Equal | 0.5 for every game. |
| Win % | log5 of the regular-season winning percentages. |
| Seeding | The better seed wins the series with a fixed probability (0.75). |

Each model implements the `GameModel` trait or the `SeriesModel` trait. A blanket implementation turns every game model into a series model.

# Outputs for each team

* Probability to reach the second round, the conference finals and the Finals (conference winner).
* Championship probability, exact and Monte Carlo, with the Monte Carlo standard error.
* Simulated title count.
* Actual playoff result.

The exact values come from a dynamic program over the bracket. They have no Monte Carlo error. Monte Carlo values are shown to validate the exact values and to give title counts and Finals matchup frequencies.

# Inputs

Ratings come from the game immediately before the playoffs (after the play-in) by default. You can select the regular-season end rating. You can override any rating by hand. Playoff games are never used as inputs.
