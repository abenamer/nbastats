---
type: Dataset
title: Player data
description: Where the players, their box-score lines in every stored game and the daily injury reports come from, how they are matched to the stored games, and how a backfill resumes.
tags: [data, players, box-scores, injuries, balldontlie]
generated: { by: "human:abenamer", at: "2026-10-25T12:00:00Z" }
sources:
  - id: balldontlie
    resource: https://www.balldontlie.io
    title: BALLDONTLIE API (players, stats and player injuries endpoints)
  - id: code
    resource: /src/application/players.rs
    title: Player sync use case
  - id: migration
    resource: /migrations/0005_players.sql
    title: Player tables
---

# Purpose

The game log says which team won. It does not say who played. The player data adds the players, every player's line in every stored game, and the league's injury report day by day. With it, a later change can ask what a team was worth with and without a player. It can also forecast with the players that are out. This change stores the data and shows it. It does not yet use it in a rating.

# Source

The data comes from the BALLDONTLIE API, the same source as the results sync, through the `PlayerSource` port that `BalldontlieSource` implements beside `ResultsSource`. Three endpoints are used: the box-score lines of one date (`/stats?dates[]=`), the team list (`/teams`, to turn a team identifier into a code) and the current injury report (`/player_injuries`). The adapter pages through each with a cursor, at most 200 pages of 100 lines, paced to the plan's quota. It maps the three abbreviations that differ from the dataset's codes (BKN, CHA, PHX). The box-score and injury endpoints need a paid BALLDONTLIE tier; the free tier serves the game results only. A fetch on a tier without them fails with the API's status, and the Data page says so.

The terms of service of BALLDONTLIE (Section 6, read on 2026-10-24) permit storing the data, deriving from it and publishing the derived results. They forbid a service that substitutes for the API. The application keeps names and team codes only; it stores no logos and no images. The data is not official league data.

The adapter's parsers are tested on payloads written from the API documentation, not on recorded answers. The feed was not read during development: the first run on a real key is the check. The injury parser needs the player's team identifier in the injury endpoint's answer, which that documentation shows.

# Tables

| Table | Key | Content |
|-------|-----|---------|
| `players` | (source, source identifier) | Name, position, height, weight, college, country, draft year, round and pick. Updated on every fetch that lists the player; a blank field never overwrites a filled one. |
| `appearances` | (game, player) | The player's line in one stored game: team, seconds played, points, rebounds (total, offensive, defensive), assists, steals, blocks, turnovers, fouls, field goals, three-pointers and free throws (made and attempted). A field the source leaves blank is null; so are the seconds of a player who did not play. |
| `box_score_fetches` | game date | One row per date fetched: when, from which source, how many lines the source gave, how many matched a stored game and how many did not. |
| `injury_reports` | (report date, player) | The player's team, status (`Out`, `Day-To-Day`, `Questionable`, and so on, as the source names it, `Unknown` when blank), the expected return date and the description, cut at 500 characters. |

Every table links by the application's identifiers (`PlayerId`, `TeamId`, the game's identifier). A game removed with its season removes its lines.

# Matching

A box-score line names its date and its player's team. The store looks up the stored games of that date and matches the line to the game its team code plays in. The game decides the team's side (home or away), not the source. A line dated another day than the fetch is not matched. A line whose team has no stored game on that date is counted as unmatched and not stored: a game the schedule does not hold, a date the source assigns differently, or a team code the dataset does not know. The count per date is on the Data page. The lines of one date are written in one transaction with the fetch record: the date's old lines are deleted first, so a second fetch leaves no stale line. An empty answer for a date that has lines changes nothing: a gap at the source must not erase a stored day. A results sync updates the season's games in place, so the lines stay attached to their games. A game the source no longer lists loses its lines with the game, and a date left without a game loses its fetch record.

An injury line names its player and team. A line whose team code the store does not know is dropped. The report of a day replaces the rows of that day only: the report is current-only at the source, so the application keeps one copy per day, and the history of a player's status is the sequence of days.

# Procedure

* **Backfill.** A backfill of a season walks the dates that have a played game in the store and no fetch record, in date order, one request per 100 lines. A run that stops (a rate limit, a failed request, a restart) has stored every date before the failure. The next run starts with the dates left, so a season of about 200 dates completes over several runs on a small quota. Removing a season removes its lines and its fetch records, so a season stored again is fetched again. The Data page's **Backfill box scores** button, `POST /api/data/players/backfill` and the command `backfill-players <season>` start one.
* **Daily sync.** After a daily results sync that completed (it marks yesterday's games as played), the application fetches the current season's unfetched dates and the dates of the last 2 days again (a date fetched while a game was still on holds partial lines). Then it stores today's injury report under today's date. After a results sync that failed, or while a manual results sync runs, the player sync waits for the next day. `PLAYER_SYNC_ENABLED=false` turns this off and keeps the buttons. The command `backfill-players <season> --injuries` does the same from an external scheduler.
* **One run at a time.** A manual run and the scheduled run share one lock; a second start answers 409. The results sync has its own lock, so a manual results sync and a manual player sync may run together. A date whose game the results sync has not yet marked played is fetched on the next run.
* **Progress.** While a run is on, `GET /api/data/players/status` gives the dates done, the current date, the lines so far, the requests made and the wait before the next request. The Data page polls it.

# Outputs

* The Data page: counts of players, lines, dates fetched and injury-report days; for a season, the dates fetched with their counts and the dates still missing; a player search with each player's seasons and game counts.
* `GET /api/players?q=`, `GET /api/players/{id}`, `GET /api/players/{id}/games?season=`, `GET /api/games/{id}/box-score` and `GET /api/injuries?date=`; see the [API reference](/reference/api.md).

# Limits

* The data starts where the source's box scores start. A season the source does not cover has fetch records with zero lines, and the page shows it.
* A player is one row per source identifier. A player the source lists under two identifiers is two rows; the application does not merge them.
* The injury report is a snapshot per day. A day the application did not run has no report, and the status between two days is not known.
* Nothing in a rating or a forecast uses the player data yet.
