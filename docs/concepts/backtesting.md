---
type: Method
title: Backtesting method
description: How pre-playoff forecasts are scored series by series and on the champion, compared with baselines and tuned without leakage.
tags: [validation, backtest, calibration]
generated: { by: "human:abenamer", at: "2026-10-21T17:30:00Z" }
sources:
  - id: code
    resource: /src/domain/backtesting/mod.rs
    title: Backtesting source
---

# Procedure

For each complete season from 2003-04 onward:

1. Rebuild the historical bracket.
2. Take the pre-playoff ratings. The information cutoff is the day before the first playoff game.
3. Compute the exact probability of each team to reach each stage under each model.
4. Score every played series: the model's probability that the actual winner beats the actual loser in a series of that round, with the series' actual game-1 host. A season gives fifteen trials.
5. Score the champion forecast: compare the sixteen championship probabilities with the actual champion. A season gives one trial.
6. Store the forecasts in the `forecasts` table with the cutoff date.

# Models

| Name | Description |
|------|-------------|
| elo | Elo with the chosen home bonus and scale. |
| equal | Equal probability among the 16 playoff teams (every game is 50/50). |
| win_pct | log5 on regular-season winning percentage through the same bracket. |
| seed | The better seed wins each series with probability 0.75. |
| elo_tuned_rolling | Elo with the home bonus and scale that minimise the series log loss on earlier seasons only. |

# Series scores

Each played series is one two-sided trial. With p the probability the model gave the actual winner:

* Brier score: (p − 1)². A coin flip scores 0.25. Lower is better.
* Log loss: −ln p. A coin flip scores 0.693. Lower is better.
* Favourite won: whether p is above one half. A series in which the model gives one half to each side has no favourite and is left out of this share; the count of series with a favourite is reported.
* Brier skill score against the equal model, a coin flip: 1 − Brier / 0.25.
* The favourite's mean probability over the series with a favourite, to read beside the share it won.
* For each round (the first round, the conference semifinals, the conference finals and the Finals): the series, the mean Brier score and log loss, the favourite's share and its mean probability.
* Paired difference against the first model of the run: the mean of (this model's score − the first model's score) over the series both scored, with its standard error. The pairing removes the variation the two models share, so the difference has a smaller error than the two means. A negative value favours this model.
* Calibration of the favourite: series are grouped by the favourite's probability (bins from one half up), and the share the favourite won is shown with a Wilson 95% interval.

A model whose ratings move during a series (a K-factor) has no exact probabilities; the backtest refuses such a model when the parameters are validated. The log loss floors the probability at 10⁻⁹, so a model that gave an upset a probability of zero scores 20.7 instead of infinity.

# Champion scores

* Brier score: Σ (p_i − y_i)² over the 16 teams of a season. A uniform forecast scores 0.9375. Lower is better.
* Log loss: −ln p for the actual champion. Lower is better.
* Favourite accuracy: how often the highest-probability team won.
* Mean probability given to the eventual champion.
* Brier skill score against the equal model.
* Calibration: forecasts are grouped by probability bin. The observed frequency is shown with a Wilson 95% interval, for the champion, the Finals, the conference finals and the second round.

The season table shows both readings for each season: the champion scores and the mean series scores with the number of series the favourite won.

# Holdout rule

Rolling tuning evaluates season t with parameters fitted on seasons before t only. The first five seasons are training data only. The criterion is the series log loss (`criterion: series`, the default) or the champion log loss (`criterion: champion`). The in-sample best grid point is reported for transparency but is not a fair score; its value is the criterion's mean log loss per trial (per series or per champion), so it compares with the model table's column of the same criterion.

# Two kinds of uncertainty

Exact probabilities have no Monte Carlo error. The standard errors on the score tables come from the spread across series (315 series in 21 seasons) or across seasons (21 champions). The champion scores are too noisy to separate the models; the series scores, and above all the paired differences, are not. Both are uncertainty about calibration, not about simulation. The series standard errors treat the series as independent trials, which they are not quite: a later round's series exist because of earlier results, and every series of a season rests on the same pre-playoff ratings, so the errors are a little small.

# Result for version 0.1.0 (2003-04 to 2023-24)

| Model | Brier | Log loss | Favourite won | Mean P(winner) |
|-------|-------|----------|---------------|----------------|
| elo (bonus 100, scale 1.0) | 0.829 ± 0.089 | 2.13 ± 0.26 | 6 of 21 | 0.222 |
| equal | 0.938 | 2.77 | 0 of 21 | 0.063 |
| win_pct | 0.811 ± 0.092 | 1.83 ± 0.22 | 6 of 21 | 0.246 |
| seed | 0.828 ± 0.044 | 1.91 ± 0.15 | 6 of 21 | 0.180 |
| elo_tuned_rolling | 0.815 ± 0.090 | 2.08 ± 0.26 | 5 of 16 | 0.205 |

Rolling tuning selects a scale of 0.75 and a home bonus of 150 from 2010-11 onward. Plain Elo is over-confident on this sample. The differences are inside the standard errors.

# Series result for version 0.1.0 (2003-04 to 2023-24, 315 series)

Measured on 2026-10-24 with the default models and grid on the stored games.

| Model | Brier | Log loss | Favourite won | Δ log loss against elo (paired) |
|-------|-------|----------|---------------|---------------------------------|
| elo (bonus 100, scale 1.0) | 0.192 ± 0.011 | 0.564 ± 0.028 | 71.1% of 315 | — |
| equal | 0.250 | 0.693 | no favourite | +0.129 ± 0.028 |
| win_pct | 0.174 ± 0.010 | 0.527 ± 0.026 | 74.4% of 301 | −0.037 ± 0.016 |
| seed | 0.197 ± 0.013 | 0.584 ± 0.028 | 73.0% of 315 | +0.020 ± 0.023 |
| elo_tuned_rolling (240 series) | 0.196 ± 0.013 | 0.580 ± 0.032 | 72.1% of 240 | −0.001 ± 0.007 |

By round, the Elo favourite won 81.0% of first-round series (168), 63.1% of conference semifinals (84), 47.6% of conference finals (42) and 71.4% of Finals (21), against mean favourite probabilities of 0.74, 0.71, 0.69 and 0.74 (the by-round table shows both). The reading:

* Plain Elo beats the coin flip and the seed model, but the log5 model on regular-season winning percentage beats Elo by 0.037 in log loss, more than two standard errors of the paired difference. The season record carries information about the playoffs that the season-end rating does not.
* Elo is under-confident in the first round and over-confident in the conference finals, where its favourite won fewer than half of the series. The champion scores cannot show this; the series scores by round do.
* Rolling tuning does not help: the tuned model is within one thousandth of plain Elo on the same 240 series. The series criterion picks the same grid point as the champion criterion (bonus 150, scale 0.75) and, on a wider grid, keeps moving to a higher bonus and a lower scale with almost no gain, so the default grid stays as it is. The gap is in the inputs, not in the two parameters.
