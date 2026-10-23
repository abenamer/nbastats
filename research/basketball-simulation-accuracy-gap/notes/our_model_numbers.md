# Our model's own numbers (computed from the application's database, 2026-10-23)

Source: the nbastats application's PostgreSQL game log (Neil Paine's continuation of the FiveThirtyEight NBA Elo archive, 1946-47 to 2024-25, loaded from `data/nba_elo_neil_paine.csv.gz`; Elo recomputed by the application: K 20, home advantage 100 Elo points, margin-of-victory multiplier, 25 percent regression toward 1505 at each season start, expansion teams at 1300). These are the application's own measurements, not published figures. Win probability per game = 1 / (1 + 10^(-(home_elo_pre + 100 - away_elo_pre)/400)), home bonus 0 for neutral-site games. Ties excluded.

## Game-level scores of the pre-game Elo forecast

| Span | Game type | Games | Favourite correct | Home win rate | Brier (per game) | Log loss (per game) |
|---|---|---|---|---|---|---|
| 1946-47 to 2002-03 | regular | 44,646 | 68.5% | 62.7% | 0.2020 | 0.5890 |
| 1946-47 to 2002-03 | playoff | 3,026 | 67.3% | 65.8% | 0.2109 | 0.6107 |
| 2003-04 to 2024-25 | regular | 26,457 | 66.0% | 58.3% | 0.2128 | 0.6136 |
| 2003-04 to 2024-25 | playoff | 1,757 | 65.6% | 62.7% | 0.2164 | 0.6221 |
| 2003-04 to 2024-25 | play-in | 25 | 68.0% | 72.0% | 0.2059 | 0.6009 |

The queries are in `scripts/our_model_numbers.sql`. The regular-season count of 26,457 since 2003-04 is 27 below the dataset's row count for the same seasons because the application classifies the 25 play-in games and 2 other games as not regular-season. Reference points: a coin flip scores Brier 0.25 and log loss 0.693; "home team always wins" scores 58.3% since 2003-04.

## Per season, regular season (2003-04 = 2004)

| Season | Games | Accuracy | Brier | Log loss |
|---|---|---|---|---|
| 2004 | 1189 | 65.2% | 0.2135 | 0.6137 |
| 2005 | 1230 | 66.4% | 0.2071 | 0.6000 |
| 2006 | 1230 | 66.3% | 0.2157 | 0.6209 |
| 2007 | 1230 | 63.9% | 0.2207 | 0.6325 |
| 2008 | 1230 | 68.7% | 0.2013 | 0.5866 |
| 2009 | 1230 | 68.9% | 0.1970 | 0.5762 |
| 2010 | 1230 | 69.1% | 0.2011 | 0.5871 |
| 2011 | 1230 | 68.3% | 0.2015 | 0.5878 |
| 2012 | 990 | 66.7% | 0.2092 | 0.6060 |
| 2013 | 1229 | 67.3% | 0.2024 | 0.5889 |
| 2014 | 1230 | 65.7% | 0.2099 | 0.6065 |
| 2015 | 1230 | 66.9% | 0.2082 | 0.6027 |
| 2016 | 1230 | 69.3% | 0.1997 | 0.5837 |
| 2017 | 1230 | 64.9% | 0.2198 | 0.6299 |
| 2018 | 1230 | 66.2% | 0.2152 | 0.6203 |
| 2019 | 1230 | 65.7% | 0.2142 | 0.6171 |
| 2020 | 1059 | 63.9% | 0.2223 | 0.6350 |
| 2021 | 1080 | 60.8% | 0.2336 | 0.6605 |
| 2022 | 1230 | 64.5% | 0.2272 | 0.6494 |
| 2023 | 1230 | 63.1% | 0.2297 | 0.6538 |
| 2024 | 1230 | 64.0% | 0.2182 | 0.6246 |
| 2025 | 1230 | 65.4% | 0.2168 | 0.6232 |

Accuracy drifts down from about 68-69% (2008-2011, 2016) to 61-65% (2021-2025). The 2020-21 season (COVID, no fans, shortened) is the worst.

## Calibration of the home win probability, regular season 2003-04 to 2024-25

| Forecast bin | Games | Mean forecast | Observed home wins |
|---|---|---|---|
| 0.0-0.1 | 13 | 0.090 | 0.077 |
| 0.1-0.2 | 295 | 0.168 | 0.186 |
| 0.2-0.3 | 1164 | 0.257 | 0.238 |
| 0.3-0.4 | 2199 | 0.353 | 0.332 |
| 0.4-0.5 | 3221 | 0.453 | 0.424 |
| 0.5-0.6 | 4307 | 0.552 | 0.503 |
| 0.6-0.7 | 5226 | 0.651 | 0.602 |
| 0.7-0.8 | 5101 | 0.749 | 0.706 |
| 0.8-0.9 | 4050 | 0.846 | 0.812 |
| 0.9-1.0 | 881 | 0.922 | 0.906 |

The forecast runs 3 to 5 percentage points above the observed home win rate in the 0.5 to 0.9 bins: the fixed 100-point home advantage (worth about 64% for equal teams) is too large for the modern league, where the home team wins 58% of regular-season games. The application's rolling backtest reaches the same conclusion from the other side for the playoffs (it selects a scale of 0.75, i.e. flatter probabilities).

## Championship forecast backtest (from docs/concepts/backtesting.md, version 0.1.0, seasons 2003-04 to 2023-24, 21 champions)

| Model | Brier over the 16 playoff teams | Log loss on the champion | Favourite won | Mean P(winner) |
|---|---|---|---|---|
| elo (bonus 100, scale 1.0) | 0.829 ± 0.089 | 2.13 ± 0.26 | 6 of 21 | 0.222 |
| equal (uniform over 16) | 0.938 | 2.77 | 0 of 21 | 0.063 |
| win_pct (log5 on regular-season record) | 0.811 ± 0.092 | 1.83 ± 0.22 | 6 of 21 | 0.246 |
| seed (better seed wins 75%) | 0.828 ± 0.044 | 1.91 ± 0.15 | 6 of 21 | 0.180 |
| elo_tuned_rolling | 0.815 ± 0.090 | 2.08 ± 0.26 | 5 of 16 | 0.205 |

The ± values are standard errors from the spread across seasons; the differences between the non-uniform models are inside them. Exact bracket probabilities are used, so there is no Monte Carlo error in these scores.
