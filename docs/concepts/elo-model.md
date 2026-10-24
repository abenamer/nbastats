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

The rating update gives the home team a bonus of 100 Elo points. This value matches the source data,[^fte-method] and it keeps the ratings on the archive's scale. Neutral games get no bonus. The 2020 playoffs in the bubble are neutral.

The home advantage of the real games is not constant. The bonus that fits the played regular-season games best was about 85 Elo points in the 2000s and is between 35 and 45 points in the 2020s. A forecast that keeps 100 points is therefore too confident about the home team. The forecasts price regular-season games with a fitted bonus instead:

* The regular-season bonus of a season is fitted on the played, non-neutral regular-season games of the previous five seasons. The fit minimises the log loss of the game forecasts over a bonus between -100 and 300 points. It needs at least 2,000 games; with fewer, or when the minimum sits at an end of that range, the bonus stays at 100. The fitted value is always inside the range a forecast accepts (-100 to 400).
* Playoff games keep 100 points. A bonus fitted on earlier postseasons does not lower the log loss of the next one. The backtest's rolling tuning stays the instrument for the playoff bonus.
* Every rating update keeps 100 points: the update of a played game, so the application still reproduces the archive's ratings, and the update of a simulated game when a forecast runs with a K-factor, so a simulated rating path moves as the real one will. Only the probability that decides a simulated game uses the fitted bonus.
* The season model version does not change: the bonus a forecast used is stored in its parameters, and a stored forecast without one used 100.

The fit is `domain::elo::fit_home_advantage`; the window and the cache are `application::home_advantage` and `App::home_advantage`. `GET /api/seasons/{season}/home-advantage` reports the bonus of a season with the seasons and the number of games it was fitted on. The Season Forecast page shows the fitted value and takes a different one; the question box's probability calculator uses the current season's value for a game. The measurement that led to this rule was made on 2026-10-23 with one-off queries over the stored games of 2003-04 to 2024-25, each season forecast with the bonus fitted on the five seasons before it; the bundle has no script that repeats it, and `GET /api/seasons/{season}/home-advantage` shows the fitted values the store gives today:

| Games | Bonus of 100 | Fitted bonus (previous five seasons) |
| --- | --- | --- |
| Regular season, 26,331 games | log loss 0.6132 | log loss 0.6091 |
| Playoffs | log loss 0.6202 | log loss 0.6208 |

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
