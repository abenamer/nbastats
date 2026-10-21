---
type: Method
title: Backtesting method
description: How pre-playoff forecasts are scored, compared with baselines and tuned without leakage.
tags: [validation, backtest, calibration]
generated: { by: "human:abenamer", at: "2026-10-21T17:30:00Z" }
sources:
  - id: code
    resource: /src/backtesting/mod.rs
    title: Backtesting source
---

# Procedure

For each complete season from 2003-04 onward:

1. Rebuild the historical bracket.
2. Take the pre-playoff ratings. The information cutoff is the day before the first playoff game.
3. Compute the exact championship probability of each team under each model.
4. Compare with the actual champion.
5. Store the forecasts in the `forecasts` table with the cutoff date.

# Models

| Name | Description |
|------|-------------|
| elo | Elo with the chosen home bonus and scale. |
| equal | Equal probability among the 16 playoff teams (every game is 50/50). |
| win_pct | log5 on regular-season winning percentage through the same bracket. |
| seed | The better seed wins each series with probability 0.75. |
| elo_tuned_rolling | Elo with the home bonus and scale that minimise log loss on earlier seasons only. |

# Scores

* Brier score: Σ (p_i − y_i)² over the 16 teams of a season. A uniform forecast scores 0.9375. Lower is better.
* Log loss: −ln p for the actual champion. Lower is better.
* Favourite accuracy: how often the highest-probability team won.
* Mean probability given to the eventual champion.
* Brier skill score against the equal model.
* Calibration: forecasts are grouped by probability bin. The observed frequency is shown with a Wilson 95% interval, for the champion, the Finals, the conference finals and the second round.

# Holdout rule

Rolling tuning evaluates season t with parameters fitted on seasons before t only. The first five seasons are training data only. The in-sample best grid point is reported for transparency but is not a fair score.

# Two kinds of uncertainty

Exact probabilities have no Monte Carlo error. The standard errors on the score tables come from the spread across seasons. With about 21 champions, this sampling noise is large. It is uncertainty about calibration, not about simulation.

# Result for version 0.1.0 (2003-04 to 2023-24)

| Model | Brier | Log loss | Favourite won | Mean P(winner) |
|-------|-------|----------|---------------|----------------|
| elo (bonus 100, scale 1.0) | 0.829 ± 0.089 | 2.13 ± 0.26 | 6 of 21 | 0.222 |
| equal | 0.938 | 2.77 | 0 of 21 | 0.063 |
| win_pct | 0.811 ± 0.092 | 1.83 ± 0.22 | 6 of 21 | 0.246 |
| seed | 0.828 ± 0.044 | 1.91 ± 0.15 | 6 of 21 | 0.180 |
| elo_tuned_rolling | 0.815 ± 0.090 | 2.08 ± 0.26 | 5 of 16 | 0.205 |

Rolling tuning selects a scale of 0.75 and a home bonus of 150 from 2010-11 onward. Plain Elo is over-confident on this sample. The differences are inside the standard errors.
