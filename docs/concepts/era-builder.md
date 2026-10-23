---
type: Concept
title: Era builder
description: How the Records page groups seasons by combined dimensions (bargaining period, rule period, league size, league, decade, competition format, dynasty era, player era) and computes the records of each group.
tags: [records, eras, era-builder, dynasty, players]
generated: { by: "human:abenamer", at: "2026-10-23T16:00:00Z" }
sources:
  - id: eras
    resource: /src/domain/eras.rs
    title: Era builder domain
  - id: players
    resource: /src/domain/players.rs
    title: Player eras
---

# Purpose

The era builder on the Records page lets you group the seasons your way and read one row per group. Pick one or more dimensions, a league and a season range; each group gets the same records as a bargaining period (see [Rule periods and bargaining periods](periods.md)): the best and weakest team-season, the titles and the franchises that won them, the number of distinct champions, the title HHI and the Elo spread. The "what changed" list under the table shows the dated changes of the periods a group belongs to, and two or more pinned groups appear side by side.

# Dimensions

A dimension maps a season of a league to one or more labels. Combining dimensions intersects them, so only combinations that occur in the data appear.

| Dimension | Labels | Source |
|-----------|--------|--------|
| CBA period | The bargaining period in force; ABA seasons are "ABA (no agreement)". | Curated table in `domain::cba`. |
| Rule period | Pre shot clock, shot clock, three-point line, zone defense, hand-check ban; the ABA on its own. | Curated table in `domain::records`. |
| League size | The number of teams of the league that season, from the schedule. | Computed. |
| League | NBA/BAA or ABA. | Computed from the team code. |
| Decade | The decade of the season's start year. | Computed. |
| Format | Early playoffs (1946-47), 16-team field (1983-84), best-of-seven throughout (2002-03), play-in era (2020-21), NBA Cup era (2023-24); ABA seasons are "ABA playoffs". | Curated table in `domain::eras`. |
| Dynasty era | A franchise's run of titles: at least 3 titles with no gap longer than 3 seasons between two of them, from its first title to its last (both thresholds adjustable). Overlapping runs share a joined label; seasons outside any run are "No dynasty". Familiar names come from a small lookup table; other runs are named after the team at its last title ("<team> dynasty"). NBA/BAA champions only; ABA seasons are "ABA (not computed)". | Computed from the champions. |
| Player era | The seasons a listed player was active, NBA/BAA and ABA apart. A season belongs to every listed player active in it, so season counts add up across rows. | Curated table in `domain::players`. |

The player table holds every league MVP since the award began in 1955-56, every Finals MVP since 1969 and George Mikan from before either award: fifty-five careers with a summary and dated milestones. The database holds no player data, so the spans are curated and need a check against a reference, as the format and rule tables do.

# Query

`GET /api/records/groups` takes `by` (comma-separated dimensions: `cba`, `rule`, `size`, `league`, `decade`, `format`, `dynasty`, `player`; at most four), `league` (`nba`, `aba` or `all`), `from` and `to` (season end years), `dynasty_titles` and `dynasty_gap`. The answer holds the query, the dynasties it found, the groups in season order (each with its labels, seasons, records and the keys of its notes) and the notes of every period once, keyed by dimension and label. A bad parameter answers 400 with a JSON message. The page's "Copy link" gives a URL that opens the same view.

# Reading the result

* A group with few seasons says little; the page greys groups under a chosen number of seasons.
* A difference between groups is an association. A period boundary coincides with expansion, other rule changes and the growth of three-point shooting, and the dimensions overlap by construction.
* Title HHI and Elo spread are defined in [Rule periods and bargaining periods](periods.md).
