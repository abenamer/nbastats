---
type: Model
title: Elo rating model
description: The game and series probability formulas, the home-court bonus and the optional rating update.
tags: [elo, probability, model]
generated: { by: "human:abenamer", at: "2026-10-21T17:30:00Z" }
sources:
  - id: fte-method
    resource: https://fivethirtyeight.com/features/how-we-calculate-nba-elo-ratings/
    title: How FiveThirtyEight calculates NBA Elo ratings
  - id: code
    resource: /src/elo/mod.rs
    title: Elo module
---

# Game probability

The probability that team A beats team B is:

    P(A beats B) = 1 / (1 + 10^((E_B − E_A) / 400))

E_A and E_B are the Elo ratings. A difference of 400 points gives odds of 10 to 1.

# Home-court bonus

The home team gets a bonus of 100 Elo points by default. This value matches the source data.[^fte-method] You can set a different value on each page. Neutral games get no bonus. The 2020 playoffs in the bubble are neutral.

# Elo scale

The playoff model can multiply the rating difference by a scale factor. A scale below 1.0 makes the forecast less confident. The backtest tunes this value.

# Series probability

A series is a state machine. It records one game at a time until one side has four wins (three wins in a best-of-five). The exact series probability is a dynamic program over the win counts. The home pattern decides which games the home-court team hosts:

| Series | Pattern |
|--------|---------|
| Best-of-seven | 2-2-1-1-1 |
| NBA Finals 1985 to 2013 | 2-3-2 |
| First round before 2003 | best-of-five, 2-2-1 |

# Rating update

Ratings are fixed during a simulated playoff run by default. You can set a K-factor. Then each simulated game updates both ratings:

    E_A' = E_A + K × (result − P(A beats B))

The update has no margin-of-victory term. Exact probabilities are not available when K is set, because the ratings depend on the simulated path.
