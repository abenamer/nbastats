---
type: Architecture Decision
title: Market lines from The Odds API, with ESPN as the fallback
description: Why the Tonight panel and the market tracking read one chosen sportsbook's lines from The Odds API when a key is set, fall back to ESPN's scoreboard, and capture only on the night's date to spare the plan's credits.
tags: [decision, market, odds, fanduel]
generated: { by: "human:abenamer", at: "2026-11-02T12:00:00Z" }
---

# ADR 0009 — Market lines from The Odds API, with ESPN as the fallback

## Context

[ADR 0006](0006-market-lines-from-espn-scoreboard.md) reads the night's lines from ESPN's scoreboard, which carries one sportsbook's lines (DraftKings) in an undocumented shape, and [ADR 0008](0008-track-the-model-against-the-market.md) stores the last read before each game's start. The maintainer bets at FanDuel and compares the model with FanDuel's lines by hand, so the stored DraftKings lines are not the ones the bets are placed at. The maintainer has bought a plan at The Odds API, a documented service that lists, for each coming game, the moneylines, spreads and totals of the sportsbooks asked for, and bills a credit per market in each request.

## Decision

* A second adapter, `adapters::odds_api::OddsApi`, implements the `MarketSource` port. One request per date asks for the NBA games that tip off on that date, Eastern time, with the configured sportsbook's moneyline, spread and total as American odds. The sportsbook is `ODDS_API_BOOKMAKER`, `fanduel` by default. The port gains a `name`, which the snapshots record as their source.
* With `ODDS_API_KEY` set, The Odds API is the first market source; ESPN's scoreboard stays the fallback when a read fails, and the only source when there is no key. The market says which source answered and when it fell back, and the snapshots keep the sportsbook and the source, so a record that mixes the two can be read apart.
* The lines cache keeps its reads by source and date, so a fallback read does not displace the first source's, and a failing first source is asked again after its one minute.
* The tracking reads by tip-off, not by the clock: only when the night is today by the league's calendar (Eastern time, `domain::calendar`, which every "today" in the application now reads), once when the date comes, to learn the tip-off times, and once in the last eleven minutes before each tip-off (the looks are ten minutes apart, at a fixed period, and the extra minute keeps a tip-off between two looks on the earlier one). Only the last capture before a game's start stands, and every game of a date tips off on that date, so any other read buys nothing and costs credits. A read with no answer records nothing and is tried again at the next look; one that priced no game is tried again after an hour. A full night is about seven reads, some 20 credits at three a read; a season's month of about 28 game nights is 550 to 600, which the free 500 credits do not quite cover and the smallest paid plan does.
* The adapter keeps the credit count from the service's answer headers. The market carries it, the panel shows it, and the tracking warns once a night when the credits left do not cover the night's remaining reads at the last read's cost.
* The key travels in the request's query string, as the service requires. The adapter drops the URL from a transport error and blanks the key in an error body before either is logged, redacts the key in `Debug` output, and the application warns at start when `ODDS_API_BASE_URL` is not `https`; the service's error messages are cut to 300 characters.
* An answer that lists games not yet started but no sportsbook for any game (a bookmaker key the service does not know, or one that does not price the league) is a failed read, so it is logged, falls back to ESPN and is tried again after a minute, instead of standing as an empty night. A started game without the sportsbook is normal: the book pulls its lines once the game is under way.

## Consequences

* The stored lines are the sportsbook's the maintainer bets at, so the lean and the side against the spread are scored at the prices a bet would have taken.
* A documented service replaces an undocumented feed as the first source; ESPN still serves when the service is down or the credits run out, and the record says which rows are whose.
* The comparison costs the service's smallest paid plan for a full month, or the free tier with a few nights missed at the month's end. The capture rule and the credit count keep the cost visible, about 550 to 600 credits a month in season; page views of the Tonight panel add three credits per read when the five-minute cache has run out.
* The lines are read up to about sixteen minutes before the start: the read comes in the last eleven minutes before the tip-off, and the lines may be five minutes old from the cache.

## Alternatives

* Reading several sportsbooks in one request was rejected for now: each sportsbook's region costs its own credits, and the maintainer bets at one.
* Buying historical odds to backfill the record was rejected: the service bills ten credits per market and snapshot for history, and the record from today forward is what the tracking needs.
* A longer cache for the service's lines, to spend fewer credits, was rejected: it would move the stored line further from the closing line, which the scoring wants.
* Reading every ten minutes through the night (the first version of this decision) was replaced the same day: about 400 credits a day, which the free tier does not cover, for lines that the next read replaced.
* Keeping ESPN as the only source was rejected: its sportsbook is not the one the bets are placed at, and its shape is undocumented.
