---
type: Runbook
title: Run the YAXEL backtest
description: How to score the emergence term over the stored seasons from the command line, read the report and decide whether it earns a place on the page.
tags: [yaxel, backtest, runbook]
generated: { by: "human:abenamer", at: "2026-11-04T09:00:00Z" }
---

# Purpose

Measure [YAXEL](/concepts/yaxel.md) before it touches a page. The backtest reads the stored games and box scores, scores the rule against the plain Elo forecast, and prints a report. It writes nothing.

# Prerequisites

* Seasons with box scores that carry BPM (the box-score backfill of the Data page), and at least one season before each sampled season to price the players from. Modern seasons qualify; the oldest are skipped with the reason.
* The application container running, or the binary with `DATABASE_URL` set.

# Procedure

1. Run the backtest over the seasons of interest. With the containers up:

   ```bash
   docker compose exec nba-app nba-elo-simulator yaxel-backtest --from 2010 --to 2026
   ```

   Each season prints a line as it is sampled or skipped, then the report. The box-score tables are rebuilt per date and per window, so a long range takes a while.
2. To keep the full report, add `--json /var/lib/nba-app/yaxel.json` (the application data volume) and copy it out with `docker compose cp nba-app:/var/lib/nba-app/yaxel.json .`.
3. Read the report:
   * **Folds**: the parameters each fold chose on its training seasons, and the test score under them. The two folds should choose similar parameters; if they do not, the rule is not stable.
   * **Seasons**: each test season's gain and the gain in standard errors (z), all games and early season.
   * **Pooled**: the test seasons together. The bar is a gain of two standard errors (z ≥ 2) on the pooled test seasons and on the early season.
4. Record the run in [ADR 0014](/decisions/0014-yaxel-an-emergence-term-backtested-before-it-is-used.md): the range, the chosen parameters and the pooled numbers. A run that clears the bar is the case for the live term; one that does not is the case against.

# Reading the gain

The gain is base Brier less YAXEL Brier per game, in thousandths. The market's lead over the model on the Model vs Market page is of the order of ten thousandths; a rule worth having moves that scale. A gain of one thousandth over a season of 1,230 games adds up to about the Brier score of one game called entirely wrong; its standard error says whether it is more than luck.
