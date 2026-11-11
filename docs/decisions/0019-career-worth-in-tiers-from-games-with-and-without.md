---
type: Architecture Decision
title: Career worth in tiers, from the box score and the games with and without a player
description: Why the Top 100 page starts each player from what the box score of his career says and lets the gap between his teams' adjusted margins with him and without him move the number, draws the ranking many times and shows tiers instead of a sorted list, why the first version that read the games alone was rejected before it shipped, and what the runs and the half-career check say about how far the list can be trusted.
tags: [decision, players, worth, career, ranking, tiers, box-score]
generated: { by: "human:abenamer", at: "2026-11-11T22:00:00Z" }
sources:
  - id: code
    resource: /src/domain/career.rs
    title: Career worth module
  - id: use-case
    resource: /src/application/career.rs
    title: Career report use case
  - id: query
    resource: /src/adapters/postgres/player_store.rs
    title: The career cells query
---

# ADR 0019 — Career worth in tiers, from the box score and the games with and without a player

## Context

The [player worth](/concepts/player-worth.md) of a season says what a team lost in the games a player missed. The maintainer asked for the same reading over whole careers: the 75 players whose teams' results changed most with them, then asked whether players such as Klay Thompson had been left off by accident, and asked for tiers, a list of 100 and a page that follows each data pull.

A first reading sorted the career gaps and cut the list at 75. That list was wrong in kind, not in detail. A career gap has a standard error of 40 Elo or more, and the true worths of the players differ by less, so a sorted list ranks the luck with the worth, and a cut at any place leaves off players who are as likely to belong as the ones kept. The honest output is a chance of belonging to each group, not a place.

A first model, `career-with-without-1.0`, did that from the games alone: it shrank each gap toward the mean by its error, drew the ranking, and showed tiers. It was run from the command line under seven floors of evidence (the first table of Runs) and was not shipped. The maintainer read its list and objected: Jusuf Nurkic was in tier 1 and Michael Jordan was 95th, in tier 5. Both places followed from the rule and not from a fault in the code.

* **Nurkic.** His gap was 226 ± 41 Elo. Portland ended 2021-22 and 2022-23 with him and other regulars out, and the rule read every team game of a season without him as an absence. The gap measured the end of those seasons.
* **Jordan.** His gap was 86 ± 44 Elo on 158 team games missed. A player who misses few games in his best seasons gives the games little to say, and the model had nothing else to read.

The games alone cannot make a list that a reader would call the best players. The box score can say what a player's statistics are worth, and the application already computes it for every season ([Box-score worth](/concepts/box-score-worth.md)).

## Decision

* **The games.** The regular-season games from 1976-77 (the first season with a box score for most games) in which the team has a box score. The playoffs are not read: the opponents and the stakes differ, and a missed playoff game is rare.
* **The stint.** One player with one team in one season, with at least 5 games played and 24 minutes a game. A reserve's games on the bench are then not read as absences. A stint runs from his first game for the team to his last, in every season. A team game before the first or after the last is not an absence, so a trade, a shutdown or an injury that ends a season adds nothing.
* **The adjusted margin.** The team's margin, plus the opponent's pre-game Elo above 1500, less a fixed home advantage of 100 Elo (none on a neutral court), at 28 Elo a point. The player's own team's rating is not used: it adapts while he is out and would hide the absence.
* **The career gap.** Inside a stint, the gap is the mean adjusted margin of the games he played less that of the games he missed. The career gap pools the gaps, each weighted by its effective number of games (played times missed over their sum), in Elo. Its error comes from the pooled variance of the margins inside the stints. A season that carries more than a third of the weight is flagged as dominant. A player with no game missed inside a stint has no career gap.
* **The career box worth.** The box-score worth of each of his seasons from 1976-77, weighted by the minutes. A season without Box Plus/Minus is not read.
* **The line.** A weighted regression, with an intercept, of the career gap on the career box worth over the ranked players with a gap. A player's weight is one over his gap's squared error plus the squared spread of the true gaps around the line, and the spread is iterated with the line. The line needs 30 players; without it no player is ranked.
* **The career worth.** The line gives each player a starting value at his career box worth. Its error holds the spread around the line and the error of the line at that box worth, with a least value of 5 Elo. The career worth is the starting value and the career gap together, each weighted by its precision. A player without a gap keeps the starting value.
* **Who is ranked.** A player with 400 games as a regular and a career box worth. There is no floor on the games missed and no group held out: a player with few absences is ranked near his box score, with the error that leaves.
* **The ranking.** 20,000 draws, from a fixed seed, add to each career worth a normal error of its own size and rank the players. A player's chances are the shares of the draws that put him in the best 10, 25, 50 and 100, and his expected rank is his mean place.
* **The tiers.** A player's tier is the smallest of those groups he is more likely than not to belong to: tier 1 for the best 10, tier 2 for the best 25, tier 3 for the best 50, tier 4 for the best 100, and tier 5 for none. The players are ordered by tier, then by expected rank. The list is the first 100.
* **The check.** Each estimate is built on half of a player's seasons (the odd years or the even years: the gap, the box worth and the line of that half alone) and scored against the career gap of the other half, in both directions. Three estimates are scored: the gap alone, shrunk as in the first model; the starting value alone; and the career worth. The report gives the correlation, the miss beyond what the errors of the target explain, and the spread of the misses over their stated errors.
* **Where it runs.** `domain::career` holds the rule as pure functions. `PlayerStore::career_cells` reads the cells in one query. `application::career` loads the box-score table of each season, computes the report on a blocking thread, and keeps it until the next import or player sync. The command `nba-elo-simulator career-worth [--min-games --draws --seed --listed --json <path>]` prints the report and was run before the page was written. The page `/top100` and `GET /api/players/career-worth` serve the report under the default settings. The model version is `career-gap-box-2.0`.

## Consequences

* The list follows the data. A new night of games or a backfilled season changes the cells and the box worths, and the first read after it computes the ranking again. The same data gives the same list, since the seed is fixed.
* The list is still noisy, and the page says so. The 100 listed players are expected to hold about 63 members of the true best 100 among the ranked players. The half-career correlation of the career worth is 0.33. A place on the list is a statement of chance.
* The list leans on the box score where a player missed few games. The typical error of a gap is 64 Elo and the spread of the true gaps around the line is 26, so for most players the starting value weighs more than the gap. The box score undervalues defence and what does not reach a statistic, and the list inherits that.
* Some places will surprise. A player with a modest gap on many absences is pulled below his box score: Kobe Bryant is 44th, Dwyane Wade 68th and Scottie Pippen 112th in the run below.
* An absence at the start or the end of a stint is outside the window. David Robinson's 1996-97, which he ended after 6 games, adds no absence; his gap (−23 ± 55 Elo) rests on 35 other games.
* The order by tier, then by expected rank, can put a player above another with a better expected rank, when the first has an even chance of a smaller group and the second does not.
* An absence is not an experiment. A player misses games when he is hurt, a team often rests others at the same time, and the absences of team-mates are not separated. The gap is against the players who replaced him on that team, so a deep team shows less for the same player.
* The first read after a data pull costs the cells query, the box-score table of each season from 1976-77, the draws and the half-career check. The command in the release build took 9 seconds in all on the maintainer's machine on 2026-11-11, the start of the container included. The reads after the first are served from memory.
* Nothing here changes a rating, a forecast or the market record.

## Alternatives

* A sorted list of the raw gaps, the first reading, was rejected: it ranks the luck with the worth, and its head is the players with the fewest games missed and the largest errors.
* The model `career-with-without-1.0` (the games alone, a floor of 400 games and 50 team games missed, each gap shrunk toward the mean) was rejected after its command-line runs, for the two places in Context. Its floors could not mend it: a stricter floor on the games missed held out more of the players a reader expects (Paul Pierce, Nikola Jokic and Karl Malone under a floor of 100), and the list became one of the players who were hurt most.
* The box-score start with the first model's whole-season window was run as a prototype. It put Jordan 7th and left Nurkic near 30th, since the ends of Portland's two seasons were still read as his absences. The window from the first game to the last replaced it.
* The season worths of the Players page, pooled over a career, were run as a prototype. The typical error was 64 Elo and the half-career correlation was 0.16 at most, which is worse than the gap alone.
* A sorted list of the career worths was rejected for the page's order: it gives one place to each player and hides that the places from 30 to 200 are close to interchangeable. The career worth is shown beside the tier.
* The box-score worth alone would make a list most readers accept, and the half-career check puts it close to the career worth (0.31 against 0.33). It was not kept alone: the games are the only part of the measure that reads what a team's results did, the check favours the two together, and a player with many absences (Stephen Curry, 228 team games missed, gap 208 ± 33) is moved a long way by them.
* A line without an intercept was rejected: the fitted intercept is 10.6 Elo, and a player with a box worth of zero is still a regular of 400 games.
* A regression of every game's margin on every player on the floor (a career adjusted plus-minus) would separate team-mates' absences. It needs the lineups of every game from 1976-77, which the store does not hold, and was deferred.
* The playoffs were left out; a later version could read them with their own weight.

## Runs

Every run is the command, 2026-11-11, on the maintainer's store (the Docker Compose database of that day: the box scores of the seasons from 1976-77 to the games of 10 November 2026), 20,000 draws, seed 1977, the first 100 listed.

### The first model, rejected

`career-with-without-1.0`, under seven floors. Every run measured 1,427 players with a gap.

| Floor: games, missed, seasons | Ranked | Held out | Short careers | Mean (Elo) | Spread (Elo) | Typical error (Elo) | Tiers 1 to 5 in the list | Expected members of the best 100 | Half-career players | Half-career correlation | Half-career spread (Elo) |
|---|---|---|---|---|---|---|---|---|---|---|---|
| 400, 50, 0 | 378 | 126 | 923 | 45.6 | 36.0 | 40.4 | 3, 5, 13, 42, 37 | 61.3 | 378 | 0.240 | 35.9 |
| 400, 100, 0 | 232 | 272 | 923 | 52.4 | 37.4 | 35.8 | 5, 8, 21, 57, 9 | 72.2 | 232 | 0.262 | 34.6 |
| 400, 30, 0 | 437 | 67 | 923 | 42.0 | 36.1 | 43.3 | 2, 4, 14, 37, 43 | 58.1 | 435 | 0.186 | 33.7 |
| 400, 50, 3 | 364 | 140 | 923 | 47.1 | 34.2 | 39.5 | 3, 6, 12, 42, 37 | 61.5 | 364 | 0.235 | 33.7 |
| 250, 50, 0 | 502 | 260 | 665 | 42.7 | 37.1 | 43.1 | 2, 4, 12, 37, 45 | 56.3 | 501 | 0.207 | 35.4 |
| 600, 50, 0 | 236 | 50 | 1,141 | 50.6 | 39.4 | 38.5 | 4, 9, 21, 53, 13 | 71.5 | 236 | 0.314 | 40.4 |
| 400, 100, 4 | 228 | 276 | 923 | 52.0 | 38.1 | 35.6 | 5, 8, 23, 55, 9 | 73.1 | 228 | 0.271 | 35.3 |

Under the floor of 400 games and 50 missed, tier 1 was Stephen Curry (210 ± 31), Jusuf Nurkic (226 ± 41) and Mitch Richmond (200 ± 38). Michael Jordan was 95th of 378 (86 ± 44, shrunk to 62 ± 28, a 40% chance of the best 100). Klay Thompson was 135th (68 ± 46, a 30% chance). John Stockton (22 team games missed) and Kareem Abdul-Jabbar (47 since 1976-77) were held out with 124 others.

### The model kept

`career-gap-box-2.0`, under the default settings.

| Measure | Value |
|---|---|
| Players with a stint | 1,543 |
| Ranked (400 games as a regular) | 505 |
| Ranked without a career gap | 2 (Mikal Bridges, Jim Chones) |
| Long careers without a box worth | 0 |
| Short careers, not ranked | 1,038 |
| Box worths read | 1976-77 to 2026-27, less 1981-82 |
| The line, over 503 players | gap = 10.6 + 0.490 (± 0.052) × career box worth |
| Spread of the true gaps around the line | 25.8 Elo |
| Mean career box worth, and the error of the line there | 65.1 Elo, 2.5 Elo |
| Spread of the career worths | 24.7 Elo |
| Typical error of a career worth | 23.0 Elo |
| Typical error of a career gap | 63.5 Elo |
| Tiers 1 to 5 in the list | 7, 9, 17, 27, 40 |
| Expected members of the best 100 | 63.1 |

The half-career check, over 988 player halves. The target is the career gap of the half the estimate did not read.

| Estimate from one half | Correlation | Miss beyond the errors (Elo) | Spread of the misses over their errors |
|---|---|---|---|
| The gap alone, shrunk as in the first model | 0.209 | 33.8 | 1.04 |
| The starting value from the box score alone | 0.311 | 28.9 | 1.00 |
| The career worth | 0.325 | 28.1 | 1.01 |

The career worth predicts the games it did not read better than the gap alone and a little better than the box score alone, and its stated errors are the size of its misses.

| Player | Rank of 505 | Tier | Career worth ± error (Elo) | Career box worth | Starting value ± error | Career gap ± error | Team games missed | Expected rank | Chance of the best 100 | Games |
|---|---|---|---|---|---|---|---|---|---|---|
| Stephen Curry | 1 | 1 | 143 ± 21 | 185 | 101 ± 27 | 208 ± 33 | 228 | 4.9 | 100% | 1,077 |
| Nikola Jokic | 2 | 1 | 143 ± 24 | 253 | 134 ± 28 | 170 ± 48 | 69 | 5.7 | 100% | 735 |
| LeBron James | 3 | 1 | 134 ± 20 | 228 | 123 ± 27 | 147 ± 28 | 184 | 7.1 | 100% | 1,632 |
| Michael Jordan | 7 | 1 | 123 ± 25 | 249 | 133 ± 28 | 85 ± 53 | 85 | 14.7 | 99% | 1,072 |
| Kevin Durant | 18 | 3 | 97 ± 21 | 196 | 107 ± 27 | 83 ± 34 | 166 | 36.8 | 94% | 1,129 |
| Paul Pierce | 25 | 3 | 90 ± 23 | 114 | 66 ± 26 | 165 ± 46 | 70 | 52.3 | 86% | 1,250 |
| John Stockton | 26 | 3 | 91 ± 26 | 158 | 88 ± 26 | 248 ± 177 | 4 | 54.0 | 84% | 1,258 |
| Kareem Abdul-Jabbar | 29 | 3 | 87 ± 24 | 151 | 85 ± 26 | 96 ± 53 | 41 | 60.2 | 82% | 936 |
| Tim Duncan | 31 | 3 | 83 ± 21 | 138 | 78 ± 26 | 91 ± 34 | 105 | 65.9 | 80% | 1,392 |
| Jrue Holiday | 37 | 4 | 77 ± 20 | 70 | 45 ± 26 | 129 ± 33 | 173 | 81.2 | 72% | 1,096 |
| David Robinson | 38 | 4 | 78 ± 24 | 188 | 103 ± 27 | −23 ± 55 | 35 | 83.0 | 71% | 987 |
| Mitch Richmond | 39 | 4 | 77 ± 23 | 74 | 47 ± 26 | 205 ± 53 | 53 | 83.8 | 70% | 912 |
| Kobe Bryant | 44 | 4 | 73 ± 21 | 149 | 84 ± 26 | 54 ± 34 | 97 | 94.9 | 64% | 1,275 |
| Dwyane Wade | 68 | 5 | 61 ± 19 | 145 | 82 ± 26 | 36 ± 29 | 187 | 135.3 | 41% | 986 |
| Scottie Pippen | 112 | 5 | 52 ± 22 | 120 | 69 ± 26 | 9 ± 40 | 79 | 179.4 | 27% | 1,076 |
| Klay Thompson | 113 | 5 | 52 ± 23 | 53 | 37 ± 26 | 112 ± 51 | 63 | 180.1 | 29% | 875 |
| Jusuf Nurkic | 152 | 5 | 47 ± 24 | 38 | 29 ± 26 | 144 ± 60 | 46 | 203.1 | 22% | 403 |

Tier 1 is Stephen Curry, Nikola Jokic, LeBron James, Joel Embiid, Magic Johnson, Larry Bird and Michael Jordan. Tier 2 is Giannis Antetokounmpo, Luka Doncic, Kawhi Leonard, Shaquille O'Neal, Paul George, Clyde Drexler, Shai Gilgeous-Alexander, Charles Barkley and Hakeem Olajuwon.

Klay Thompson is measured and ranked, and is outside the list on his numbers: a career box worth of 53 Elo and a gap of 112 ± 51 give 52 ± 23, a 29% chance of the best 100. The window took Nurkic's gap from 226 ± 41 on the whole seasons to 144 ± 60 inside his stints, and his box worth of 38 Elo leaves him 152nd. Jordan's gap is 85 ± 53 on 85 team games missed inside his stints, with more than a third of its weight in 1985-86, and his box worth of 249 Elo puts him in tier 1.
