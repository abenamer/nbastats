---
type: Architecture Decision
title: Backfill the market record from the sportsbook's history
description: Why the market record is filled in, for the nights before the tracking was on and for the seasons before, from The Odds API's stored snapshots of the sportsbook's lines read before the tip-off, beside the stored pre-game Elo alone under its own model version, and how the plan's credits are spent on it.
tags: [decision, market, tracking, odds, backfill]
generated: { by: "human:abenamer", at: "2026-11-05T20:00:00Z" }
---

# ADR 0018 — Backfill the market record from the sportsbook's history

## Context

The market record ([ADR 0008](0008-track-the-model-against-the-market.md)) starts on the day the tracking was first on, 2 November 2026: 92 games of 2026-27 had been played by then with no line stored, and the seasons before have none. [ADR 0009](0009-market-lines-from-the-odds-api.md) rejected buying the service's history, at ten credits a market and snapshot. The maintainer, wanting the comparison over years rather than weeks, bought a month of The Odds API's paid plan, 20,000 credits, and asked how far back the record could go. The service's history holds the featured markets from June 2020, at ten-minute intervals, and at five-minute intervals from September 2022. Three nights' snapshots read on 2026-11-05 showed FanDuel's moneylines there from the 2020-21 season and its spreads and totals from 2022-23.

## Decision

* A port, `MarketHistory`, gives a past night's lines: the tip-off times of a date's games as the service listed them at a time (one credit), and the lines of the date's games as they stood at a time, the latest snapshot at or before it, in the markets named (ten credits a market). `adapters::odds_api::OddsApi` implements it beside `MarketSource`; ESPN's scoreboard keeps no history. A history read asks for every sportsbook of the US region at the same price, and a game takes the configured sportsbook's line when it has one, else the first sportsbook with a moneyline, so a night FanDuel did not price is not lost. Each row names its sportsbook, and the source `the-odds-api-historical`.
* The command `nba-elo-simulator market-backfill --from <date> --to <date> [--per-tip-off] [--markets h2h,spreads,totals] [--dry-run]` fills the record for the nights in the range that have stored regular-season games without a snapshot. For each night it reads the tip-off times at ten in the morning Eastern, then the lines fifteen minutes before the night's first tip-off, or before each tip-off time with `--per-tip-off`, as the live capture reads them. The store's rule keeps the last capture before each game's start, so a live row stands, and a repeated run costs nothing for a night already whole. A dry run lists the nights and the estimated cost. A run counts what its requests cost by the service's rates, whether or not the service answered, and stops before a request that would pass `--max-credits` or that the credits left at the service do not cover; it also stops when the service refuses a request (a key not on a paid plan, a date outside the history, the rate limit) or when five nights fail in a row, and a night gets at most sixteen reads whatever the service lists.
* The model's side of a backfilled row is the stored pre-game Elo alone, with the season's fitted home bonus and no absence priced, under its own model version (`elo-stored-1.0` with the margin model). A past night has no injury report to price as the Tonight page prices tonight's (the stored reports begin on 26 October 2026), and the record keeps the versions apart, so the live model's rows and the Elo-only rows read as two records on one page. The expected margin, the lean, the side against the spread and the flagged bets are scored as for any row.
* The budget, as the maintainer chose to spend it: the six seasons 2020-21 to 2025-26 at one moneyline read a night, about 1,800 credits a season, and the thirteen missing nights of 2026-27 at the live capture's fidelity, every tip-off time with the three markets, about 2,000 credits; about 7,000 credits stay for the live tracking's 550 to 600 a month.

## Consequences

* The record reaches back six seasons for the moneyline comparison, the Brier score and the lean, which is where the verdict of [ADR 0013](0013-call-the-model-against-the-market-by-the-paired-brier-gap.md) rests. The spread and the total are there for the live rows and for 2026-27's backfilled nights. A backfilled row carries another sportsbook's line where FanDuel had none; the sportsbook is on each row.
* A line read a quarter of an hour before the night's first tip-off is hours before the later games' tip-offs: for them, closer to an opening line than to a closing one. The per-tip-off reading costs five times as much and was kept for the current season.
* The Elo-only rows judge the stored rating, not the live model: a team's absences on the night are not priced, so those rows are a little harder on the model than the live ones. The version tag says which is which, the summaries per version keep them apart, and the overall summary mixes them.
* The credits are spent once: the history does not change, and a night read whole is never read again.

## Alternatives

* Reconstructing the live model's number as of each past night, from the day's report, the full-strength rating and the worth tables as they stood, was deferred: the reports exist for eleven days, the tables as of a date would have to be rebuilt per night to keep later games out, and the pricing path reads today throughout. It is the refinement if the Elo-only rows prove too blunt for the current season.
* Reading every tip-off time of the past seasons was rejected on cost: about 8,400 credits a season for the moneyline alone, 25,000 with the three markets.
* Reading the spreads of the past seasons was rejected: FanDuel's history has none before 2022-23, the verdict rests on the moneyline, and the credits bought two more seasons instead.
* Letting the record grow from 2 November alone, as [ADR 0009](0009-market-lines-from-the-odds-api.md) decided, was set aside by the maintainer: a verdict at two standard errors needs hundreds of games, and the history gives thousands at once.

## Runs

| Run | Range | Reading | Nights read | Rows written | Games unpriced | Credits used (left) |
|---|---|---|---|---|---|---|
| (to be recorded) | | | | | | |
