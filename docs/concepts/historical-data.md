---
type: Dataset
title: Historical data
description: Sources, coverage, franchise identity and game classification of the imported Elo game log.
tags: [data, elo, import]
generated: { by: "human:abenamer", at: "2026-10-21T17:30:00Z" }
sources:
  - id: paine
    resource: https://github.com/Neil-Paine-1/NBA-elo
    title: Neil Paine's NBA Elo archive (FiveThirtyEight format)
    last_modified: "2025-04-14T22:32:01Z"
  - id: fte-elo
    resource: https://github.com/fivethirtyeight/data/tree/master/nba-elo
    title: FiveThirtyEight nba-elo (nbaallelo.csv, 1946-47 to 2014-15)
  - id: fte-forecasts
    resource: https://github.com/fivethirtyeight/data/tree/master/nba-forecasts
    title: FiveThirtyEight nba-forecasts (links to projects.fivethirtyeight.com)
---

# Sources examined

| Source | Status | Use |
|--------|--------|-----|
| Neil Paine's archive `nba_elo.csv` | Available. Two rows per game (one per team), 1946-47 to the 2024-25 regular season. | Bundled with the application and used for refresh.[^paine] |
| FiveThirtyEight `nbaallelo.csv` | Available. Ends with 2014-15. | Supported import layout.[^fte-elo] |
| FiveThirtyEight `nba_elo.csv` on projects.fivethirtyeight.com | Host no longer serves the file. | Supported import layout when you have a copy.[^fte-forecasts] |

The bundled file is `data/nba_elo_neil_paine.csv.gz` (SHA-256 of the uncompressed file: `9a485bac…` commit `9a485bacef9f18f6a8e3ec671f13b9609386f0fa`, 2025-04-14).

# Coverage

* 1946-47 to 2024-25: 75,913 games.
* Playoff records are present for 1946-47 to 2023-24. The bracket model resolves the champion from the games for 1960-61 to 1966-67 and 1983-84 onward. For the other seasons (byes and formats the model does not resolve) the champion and runner-up come from the curated table in `src/domain/reference.rs`, marked `curated`.
* 2024-25 has the regular season only. Its champion (Oklahoma City) comes from a curated table, marked `curated`.
* Bracket reconstruction and forecasts cover 2002-03 and later, because conference alignment data starts there.
* Seasons after 2024-25 are not in the archive. The application can compute them from results, or project them. See [Elo continuation](elo-continuation.md).

# Franchise identity

A franchise is the organisation through relocations and name changes. The mapping follows FiveThirtyEight's `fran_id`. Examples: SEA and OKC are one franchise; NJN and BRK are one franchise; the 1988-2002 Charlotte Hornets belong to the New Orleans Pelicans lineage; the 2004-05 Charlotte Bobcats are a separate franchise. The full table is in `src/reference.rs`.

# Game classification

| Type | Rule |
|------|------|
| regular | Counts in the standings. |
| play_in | 2020-21 and later: games beyond each team's regular-season length. 2019-20: the games of 2020-08-15. |
| cup_final | NBA Cup final (2023-24 and later): the only game on its December date, both teams finish with 83 games. Does not count in the standings. |
| playoff | Flagged in the source. |

Elo ratings flow through every game. The pre-playoff rating is the rating after the last non-playoff game.

# Import rules

* Each game has a unique key (date, home team, away team, sequence). Repeated imports update changed rows and skip unchanged rows.
* The application stores the file checksum, source, row counts and timestamps for each import.
* Rows with unknown team codes or bad values are counted and reported, never silently dropped.
* Each game records its Elo source: `imported` from an archive file, or `computed` by the application. An archive import replaces computed games. A computed game never replaces an imported game.
