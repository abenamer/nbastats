---
type: Runbook
title: Backfill the market record
description: How to fill the Model vs Market record from The Odds API's history for the nights the live capture did not see, what it costs in credits, and how to read the result.
tags: [market, odds, backfill, runbook]
generated: { by: "human:abenamer", at: "2026-11-05T20:00:00Z" }
---

# Purpose

Fill the market record ([Market tracking](/concepts/market-tracking.md), Backfill) for the nights the live capture did not see: the games before the tracking was on, and the seasons before. The rows carry the sportsbook's lines as they stood before the tip-off, beside the stored pre-game Elo alone, under their own model version. See [ADR 0018](/decisions/0018-backfill-the-market-record-from-the-sportsbooks-history.md).

# Prerequisites

* `ODDS_API_KEY` set to a key on a paid plan: the service does not serve its history on the free plan. The sportsbook is `ODDS_API_BOOKMAKER` (FanDuel by default); a game it did not price takes another US sportsbook's line, named on the row.
* The seasons' games stored (the results sync), so the nights and the teams are known.
* Credits to spend. A night's listing costs one credit; a read of the lines costs ten per market. One moneyline read a night is about 1,800 credits a season; a read before every tip-off time with the three markets is about 25,000.

# Procedure

1. See what a range would cost, without reading anything:

   ```bash
   docker compose exec nba-app nba-elo-simulator market-backfill --from 2020-12-22 --to 2021-05-16 --dry-run
   ```

   Each night prints with its games and whether it would be read. The answer's `estimated_credits` is the cost by the estimate: one read a night, or five with `--per-tip-off`.
2. Run it. The past seasons at one moneyline read a night:

   ```bash
   docker compose exec nba-app nba-elo-simulator market-backfill --from 2020-12-22 --to 2026-06-30
   ```

   The current season's missing nights at the live capture's fidelity, every tip-off time with the three markets:

   ```bash
   docker compose exec nba-app nba-elo-simulator market-backfill --from 2026-10-20 --to 2026-11-01 --per-tip-off --markets h2h,spreads,totals
   ```

   Each night prints its tip-off times, its reads, the rows written, the games left without a snapshot and the credits left. Add `--max-credits 11000` to cap what the run may spend, counted by the service's rates (a listing one, a read ten per market) whether or not the service answers; the run stops before the request that would pass it, when the service's credits do not cover the next read, when the service refuses a request, or when five nights fail in a row. Run it again later over the same range to go on, since a night whose games all have a snapshot costs nothing. The estimate of a dry run is a floor for a per-tip-off run: it counts five tip-off times a night, and a busy night has seven or eight.
3. Read the Model vs Market page. The backfilled rows appear under the version `elo-stored-1.0 margin-28-elo-1.0`, apart from the live model's. The summaries per version judge each on its own; the overall summary mixes them.
4. Record the run in [ADR 0018](/decisions/0018-backfill-the-market-record-from-the-sportsbooks-history.md): the range, the reading, the nights, the rows, the games left unpriced and the credits.

# Notes

* Only the last capture before a game's start is kept, so a live row is never replaced by a backfilled one.
* The lines are read fifteen minutes before the night's first tip-off, or before each tip-off with `--per-tip-off`. For the later games of a night read once, the line is hours before their start.
* A night the service lists no game for, or whose lines could not be read, is noted in the answer and left for a later run. A per-tip-off night the budget cut short keeps the lines of the reads it got, the first read's for its later games, and a later run reads it as whole; its note in the answer says so.
