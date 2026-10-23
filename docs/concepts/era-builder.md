---
type: Concept
title: Era builder
description: How the facets on the Records page replay every record table over the seasons of chosen eras (bargaining period, rule period, league size, league, decade, competition format, dynasty era, player era, themed era), and how the group panel computes one row of records per combination of dimensions.
tags: [records, eras, era-builder, facets, dynasty, players, themes]
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

The Records page is a faceted search over the seasons. The panel at its top lists every era the data offers along nine dimensions; a pick keeps the seasons of that era and every table on the page is computed again over them: the highest and lowest ratings, the records by era, league, size, decade and bargaining period, East against West, the divisions, the champions, the rises and falls, the single-game swings, the matchups, the clankers and the roads to the title. The "LeBron James" chip, for example, replays the whole page over the seasons he was active. A banner names the picks, the seasons they select, the records of the selection (mean season-end Elo, Elo spread, best and weakest team-season, the titles, the number of distinct champions and the title HHI) and what changed in those periods, and the page's address follows the picks, so a link opens the same view.

Within one dimension the picks add up: a season matches when it carries any of them (Larry Bird and Magic Johnson together select the seasons either was active). Across dimensions they intersect: a season must match every dimension picked (LeBron James and the hand-check ban select his seasons from 2004-05). A combination no season matches shows an empty page under a banner that says so.

Below the facets, the group panel ("Compare groups") takes the other direction: pick dimensions to combine and read one row of records per combination that occurs, within the facets picked above. Each row carries the same records as a bargaining period (see [Rule periods and bargaining periods](periods.md)): the best and weakest team-season, the titles and the franchises that won them, the number of distinct champions, the title HHI, the mean Elo and the Elo spread; the "what changed" list under the table shows the dated changes of its periods, and two or more pinned rows appear side by side.

# Dimensions

A dimension maps a season of a league to one or more labels. A facet is one label along a dimension, written `dimension:label` in a query.

| Dimension | Labels | Source |
|-----------|--------|--------|
| CBA period | The bargaining period in force; ABA seasons are "ABA (no agreement)". | Curated table in `domain::cba`. |
| Rule period | Pre shot clock, shot clock, three-point line, zone defense, hand-check ban; the ABA on its own. | Curated table in `domain::records`. |
| League size | The number of teams of the league that season, from the schedule. | Computed. |
| League | NBA/BAA or ABA. | Computed from the team code. |
| Decade | The decade of the season's start year. | Computed. |
| Format | Early playoffs (1946-47), 16-team field (1983-84), best-of-seven throughout (2002-03), play-in era (2020-21), NBA Cup era (2023-24); ABA seasons are "ABA playoffs". | Curated table in `domain::eras`. |
| Dynasty era | A franchise's run of titles: at least 3 titles with no gap longer than 3 seasons between two of them, from its first title to its last. The facets, their listing and every scope use these thresholds; the group panel's thresholds shape its group rows only. As a facet each run is its own label and a season in two overlapping runs matches both; as a group label overlapping runs share a joined label. Seasons outside any run are "No dynasty". Familiar names come from a small lookup table; other runs are named after the team at its last title ("<team> dynasty"). NBA/BAA champions only; ABA seasons are "ABA (not computed)". | Computed from the champions. |
| Player era | The seasons a listed player was active, NBA/BAA and ABA apart. A season belongs to every listed player active in it. | Curated table in `domain::players`. |
| Themed era | A span named for how the game was played or followed: the Reddit NBA era from 2008-09 and the three-point dominance era from 2015-16. Themes overlap, so a season belongs to every theme active in it; ABA seasons belong to none. | Curated table in `domain::eras`. |

The player table holds every league MVP since the award began in 1955-56, every Finals MVP since 1969 and George Mikan from before either award: fifty-five careers with a summary and dated milestones. The database holds no player data, so the spans are curated and need a check against a reference, as the format, theme and rule tables do.

# Query

`GET /api/records/facets` takes no parameters and lists every dimension with the labels present in the data, each with its first and last season, its season count and the summary of its period or player when there is one, plus the number of facets a query may carry. The page refuses a pick past that number and drops the facets of a link beyond it, with a note.

`GET /api/records` takes `facet` parameters, repeated, each `dimension:label` (at most eight, over the dimensions `cba`, `rule`, `size`, `league`, `decade`, `format`, `dynasty`, `player`, `theme`), and answers the record tables over the selected seasons with `scope`: the facets, the distinct seasons, the first and last season, the leagues, the records of the selection (`stats`, the same fields as a group row) and the notes of the facets that name a period, a player or a theme. Without facets `scope` is null and the tables cover every season. The game-level tables (swings, matchups, clankers, roads) are ranked by the store within the selected seasons, so a swing outside them never appears. A label is matched exactly after trimming; one no season carries answers 400 with a JSON message. Other parameters are ignored.

`GET /api/records/groups` takes `by` (comma-separated dimensions; at most four), `league` (`nba`, `aba` or `all`), `from` and `to` (season end years), `dynasty_titles` and `dynasty_gap` (for the group rows), and the same `facet` parameters; a scalar parameter given twice keeps its last value. The answer holds the query, the scope when there are facets, the dynasties it found, the groups in season order (each with its labels, seasons, records and the keys of its notes) and the notes of every period once, keyed by dimension and label. The page's "Copy link" gives a URL that opens the same view.

# Reading the result

* A scope or a group with few seasons says little; the page greys groups under a chosen number of seasons.
* A difference between eras is an association. A period boundary coincides with expansion, other rule changes and the growth of three-point shooting, and the dimensions overlap by construction.
* Title HHI, mean Elo and Elo spread are defined in [Rule periods and bargaining periods](periods.md).
