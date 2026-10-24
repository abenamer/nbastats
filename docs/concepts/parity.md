---
type: Model
title: Parity profile
description: How the Records page measures, season by season, how far the champion, the finalists, the final four and the playoff field stood above their league, in Elo points and in standard deviations.
tags: [parity, records, elo, z-score]
generated: { by: "human:abenamer", at: "2026-10-24T02:00:00Z" }
sources:
  - id: code
    resource: /src/domain/parity.rs
    title: Parity module
---

# Purpose

"Was the league balanced that year?" has several answers: how far the champion stood above everyone, how far the two finalists did, how far the four conference finalists did, and how far the whole playoff field did. The parity profile gives all four for every season on one scale, so a season in 1962 and a season in 2022 can be compared, and so a group of seasons (an era, a bargaining period, a facet selection) has one parity figure.

# Measure

Every team's rating before the playoffs is the input (the season's last regular-season or play-in game). A season and league has a profile when it has some postseason information (a champion or a team labelled as a playoff team) and at least eight rated teams with a full season. A full season is at least 80 percent of the median number of games of the league's teams that season, playoff games included. This is the rule the conference tables use, so a team that folded in the autumn does not widen the spread. A season in progress has ratings but no postseason, so it has no profile. For each profiled season and league:

1. The league mean and the league spread: the mean and the population standard deviation of the pre-playoff ratings.
2. For each tier, the gap: the tier's mean rating minus the league mean, in Elo points.
3. The z-score: the gap divided by the league spread. This is the reading that carries across seasons. A gap of 100 Elo points means more in a tight league than in a spread-out one, and the spread changes with the league's size and era: about 82 points in the 1950s, 100 to 115 in the 1960s to 1980s, 119 to 132 since.
4. The gap to the rest: the tier's mean minus the mean of every other team, for the reader who wants the champion against the field it beat rather than against a mean it is part of. It is 0 when the tier is the whole league.
5. The champion's rank: its place among the league's pre-playoff ratings, 1 for the best.

The tiers:

| Tier | Teams | Condition |
|------|-------|-----------|
| Champion | 1 | the archive names a champion among the rated teams |
| Finalists | 2 | exactly two teams are labelled as having reached the Finals or won |
| Final four | 4 | exactly four teams are labelled as having reached the conference finals or further |
| Playoff field | 4 or more | the teams labelled as having reached the first round or further |

A tier whose labelled count is not the expected one is left out of that season. The round labels come from the reconstruction of each season's postseason from the game log, and they are incomplete in two places. The early BAA and NBA formats (byes, three divisions, a semifinal round for the division winners) give the finalists in only 2 of the 14 seasons to 1959-60 and the final four in 3. The seasons the ABA shared with the NBA, 1967-68 to 1973-74, give neither tier for the NBA and the finalists for the ABA in 1970-71 only; from 1974-75 the NBA has both. The champion comes from the curated list of champions and is present in every NBA/BAA season. The ABA has a champion label in 1970-71, 1973-74 and 1974-75 only, and a profile from 1967-68 to 1974-75 (1975-76 has seven rated teams of nine). The field is present in every season but 2024-25, whose results came from the results source without playoff labels: its champion is in the curated list, its other teams are labelled as having missed the playoffs, and so it has no field, finalists or final four until its labels are imported.

The parity figure of a group of seasons is the mean z of each tier over the seasons that have the tier. The Records page shows it for the facet selection (in the banner), for every bargaining period and for every group row of the era builder (the champion's z).

# Reading the result

* A champion near z = 2.5 towered over its league: 1985-86 Boston (2.56, 267 Elo above the mean) and 2016-17 Golden State (2.52, 265). A champion below 1 won from the pack: 2022-23 Denver (0.43, rank 11), 1994-95 Houston (0.26, rank 13) and 1957-58 St. Louis (0.01, rank 5).
* The figures below were measured on 2026-10-24 over the 87 profiled seasons: 79 NBA/BAA seasons from 1946-47 to 2024-25 and 8 ABA seasons from 1967-68 to 1974-75. The champion's z averages 1.35 (152 Elo above the league mean), the finalists' 1.15, the final four's 1.06 and the playoff field's 0.59. Of the 82 seasons with a champion, the champion had the league's best pre-playoff rating in 39, and its mean rank is 2.8. The champion's z varies across seasons with a standard deviation of 0.53: 10 champions at 2 or more and 23 below 1.
* By decade (NBA/BAA; a count in brackets says how many seasons have the tier when not all do):

| Decade | Seasons | Spread | Champion z | Finalists z | Final four z | Field z | Champion best-rated |
|--------|---------|--------|------------|-------------|--------------|---------|---------------------|
| 1940s | 4 | 111 | 1.21 | 0.63 (1) | 0.97 (1) | 0.51 | 2 of 4 |
| 1950s | 10 | 82 | 1.19 | 0.81 (2) | 0.19 (2) | 0.34 | 6 of 10 |
| 1960s | 10 | 105 | 1.37 | 1.08 (7) | 0.75 (7) | 0.51 | 6 of 10 |
| 1970s | 10 | 100 | 1.12 | 1.02 (6) | 1.11 (6) | 0.69 | 4 of 10 |
| 1980s | 10 | 115 | 1.65 | 1.36 | 1.12 | 0.60 | 6 of 10 |
| 1990s | 10 | 132 | 1.55 | 1.38 | 1.23 | 0.70 | 6 of 10 |
| 2000s | 10 | 119 | 1.37 | 1.10 | 1.17 | 0.73 | 2 of 10 |
| 2010s | 10 | 122 | 1.39 | 1.17 | 1.13 | 0.71 | 4 of 10 |
| 2020s | 5 | 123 | 1.18 | 1.04 (4) | 0.97 (4) | 0.73 (4) | 2 of 5 |

* The 1980s and 1990s are the decades of the towering champion (z 1.65 and 1.55). The 1970s and 2020s are the decades of the pack (1.12 and 1.18). The playoff field has drifted up since the 1950s, from 0.34 to about 0.7: the teams that miss the playoffs sit further below the mean than they did.
* The champion's z and its rank say different things. A champion of rank 1 at z 1.2 won a balanced season from the top. A champion of rank 4 at z 1.5 won a spread-out season from below the top.

# Limits

* The z-score depends on the spread of the whole league, so an expansion season (new teams enter at 1300) widens the spread and lowers every z that year.
* The tiers come from the round labels the application reconstructs from the game log, season by season. Where the labels are incomplete, the tier is absent, not estimated.
* The pre-playoff rating is one number per team; injuries, trades and rest in the playoffs are not in it.
