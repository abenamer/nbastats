---
type: Architecture Decision
title: A declared absence for Michael Jordan's retirement
description: Why the career worth reads the games Chicago played during Michael Jordan's retirement from October 1993 to March 1995 as games he missed, by a declaration made by hand for him alone, what the stint rule could not see, what the run says the declaration changes (his worth goes down, not up), and why no other player has one.
tags: [decision, players, worth, career, ranking, tiers, exception]
generated: { by: "human:abenamer", at: "2026-11-11T13:38:00Z" }
sources:
  - id: code
    resource: /src/domain/career.rs
    title: Career worth module
  - id: list
    resource: /src/domain/declared_absences.rs
    title: The list of declared absences, moved here by ADR 0021
  - id: after
    resource: /docs/decisions/0021-declared-absences-for-long-injuries-and-retirements.md
    title: ADR 0021, which makes the declaration general
  - id: query
    resource: /src/adapters/postgres/player_store.rs
    title: The career cells query
  - id: use-case
    resource: /src/application/career.rs
    title: Career report use case
  - id: before
    resource: /docs/decisions/0019-career-worth-in-tiers-from-games-with-and-without.md
    title: ADR 0019, the model this decision amends
---

# ADR 0020 — A declared absence for Michael Jordan's retirement

**Amended on 2026-11-11 by [ADR 0021](/decisions/0021-declared-absences-for-long-injuries-and-retirements.md).** The list is no longer for one player. ADR 0021 declares each season that meets one criterion, for 107 players, and it replaces the part "No other player" of the decision below and the alternative "A general rule". The list moved to `domain::declared_absences`. The two rows for Michael Jordan stay. His 1993-94 row is read inside a stint of another season, and under ADR 0021 it counts as fewer games, so the numbers of `career-gap-box-2.1` below are not those of the page. The text below is the decision as it was made.

## Context

[ADR 0019](/decisions/0019-career-worth-in-tiers-from-games-with-and-without.md) reads an absence as a team game between a player's first game and his last game of a stint. The store has no rosters of past seasons. Thus the rule cannot tell a player who sat out from a player who was on another team, or on no team. The window from the first game to the last is the safe reading.

The window hides Michael Jordan's first retirement. He left Chicago on 6 October 1993 and played again on 19 March 1995.

* In 1993-94 he has no game, thus no stint. The 82 games Chicago played add nothing.
* In 1994-95 his stint starts on 19 March 1995. The 65 games Chicago played before that date are before his first game, and add nothing.

Under the model `career-gap-box-2.0` his career gap rests on 85 games missed, none of them in the retirement. The maintainer asked why the season of the retirement does not count as an absence, and then asked for an exception for Jordan alone.

The case differs from the cases the window protects against. He was not traded. He was not on another roster. Chicago kept his rights, and he came back to the same team and the same coach.

## Decision

* **A declared absence.** A declaration says, by hand, what the store cannot: every game a team played in one season is read as inside one stint of one player with that team. The games he played in that season, if any, stay games played. The other games become games he missed in that stint. A game inside the stint's own window is read once.
* **The list.** `DECLARED_ABSENCES` in `domain::career` is the whole list. It holds two rows, both for Michael Jordan with Chicago:

  | Games read | Stint that receives them | Games added as missed |
  |---|---|---|
  | The 82 games of 1993-94 | 1992-93, the season before | 82 |
  | The games of 1994-95 | 1994-95, his 17 games from 19 March 1995 | 65 |

* **No other player.** The list has no other row. This is the maintainer's choice, and it is not a rule that the data applies. The page and the report say so.
* **Keys.** A row names the player by the source's key (`balldontlie`, 2931) and the team by its code (`CHI`). The database's own keys differ between stores. A row whose player, team or stint is not in the store adds nothing.
* **Where it runs.** `CellRule::declared` carries the list to the store, and `PlayerStore::career_cells` applies it in the same query. The report shows the rows under `rule.declared`, and a player's row carries `declared_absence`, the text for the page, or null. The Note column of the Top 100 page shows the text. The command `nba-elo-simulator career-worth` prints one line for each declaration.
* **Version.** The model version is `career-gap-box-2.1`. The rest of the model of ADR 0019 is unchanged.

## Consequences

* **His worth goes down, not up.** The run below puts his career worth at 115 ± 22 Elo, from 123 ± 25. He stays in 7th place and moves from tier 1 to tier 2: his chance of the best 10 goes from 58% to 46%.
* **Why.** His career gap hardly moves (88 Elo from 85), because Chicago stayed a good team without him: 55 wins in 1993-94. The error of the gap falls from 53 to 35 Elo, since it now rests on 232 games missed. His starting value from the box score is 132 Elo. A firmer gap of 88 pulls the worth further below 132 than a loose gap of 85 did.
* **He is measured with more evidence than his peers.** A player who missed a whole season with an injury (Klay Thompson, 2019-20 and 2020-21), or who retired and came back (Magic Johnson, 1991-92 to 1994-95), is still read by the window alone. A comparison of Jordan with such a player is not like for like, and the Note column marks his row.
* **The 1993-94 games are read against another season.** The 82 games of 1993-94 are compared with the games he played in 1992-93. The roster, the schedule and the league around the team changed between the two seasons. Inside a normal stint the two sides are of the same season.
* **The declaration is a judgement about history.** It says that he was Chicago's player the whole time and that the team around him was near enough the same. The store cannot check either statement.
* **Other players move a little.** His gap is one of the 503 points of the line from the box worth to the gap. The line moves, and each starting value with it. The largest change of another player's worth is 0.7 Elo. No other player changes tier, and the same 100 players are on the list.
* Nothing here changes a rating, a forecast or the market record.

## Alternatives

* **No exception** keeps every player under one rule. It leaves the best-known absence in the sport unread, and the maintainer asked for it to be read.
* **A general rule** for a whole season out between two seasons with the same team would read Jordan's 1993-94 and also Klay Thompson's two seasons. The store cannot tell a player who sat out from a player who was in another league or on no roster, and a rule that guesses would read some such seasons wrongly. It was not run. It is the like-for-like way to do this, and it stays open.
* **A general rule for the whole season of a one-team player** would read the 65 games of 1994-95. It would also read the end of a season after an injury, which ADR 0019 took out of the window on purpose (the prototype with the whole-season window put a player near 30th on the ends of two seasons).
* **The 1993-94 games read inside the 1994-95 stint**, against his 17 games after the return. Rejected: 17 games played would carry 147 games missed, and the 1992-93 team is the closer comparison for the team of 1993-94.
* **A hand-set worth or a hand-set place for one player** was rejected. The declaration changes what is read, not the result, and the result went against the expectation.

## Runs

Both runs are of 2026-11-11, on the maintainer's store (the Docker Compose database of that day), 20,000 draws, seed 1977, the first 100 listed. The run of `career-gap-box-2.0` is the report the running application served. The run of `career-gap-box-2.1` is the command `nba-elo-simulator career-worth --json` in the release build, before the change was on a page.

### Michael Jordan

| Measure | 2.0 | 2.1 |
|---|---|---|
| Place | 7 | 7 |
| Tier | 1 | 2 |
| Career worth (Elo) | 122.6 ± 24.5 | 114.9 ± 21.7 |
| Starting value from the box score (Elo) | 132.6 ± 27.6 | 131.6 ± 27.6 |
| Career gap (Elo) | 85.5 ± 53.2 | 88.0 ± 35.0 |
| Games played inside the stints with a game missed | 317 | 334 |
| Games missed | 85 | 232 |
| Dominant season | 1985-86 | 1992-93 |
| Expected rank | 14.7 | 18.1 |
| Chance of the best 10 | 58.4% | 46.4% |
| Chance of the best 25 | 84.5% | 78.6% |
| Chance of the best 100 | 99.1% | 99.0% |

Chicago's results in the games the declaration reads, as raw margins without the adjustment, from a query on the same store on the same day:

| Games | Count | Wins | Mean margin (points) |
|---|---|---|---|
| 1992-93, with him | 78 | 56 | +6.46 |
| 1992-93, without him | 4 | | +3.00 |
| 1993-94, without him | 82 | 55 | +3.09 |
| 1994-95, before his return | 65 | 34 | +4.29 |
| 1994-95, from his return | 17 | 13 | +6.88 |

### The whole report

| Measure | 2.0 | 2.1 |
|---|---|---|
| Players with a stint | 1,543 | 1,543 |
| Ranked | 505 | 505 |
| Ranked without a career gap | 2 | 2 |
| Short careers, not ranked | 1,038 | 1,038 |
| The line, over 503 players | gap = 10.6 + 0.490 (± 0.052) × career box worth | gap = 10.9 + 0.485 (± 0.052) × career box worth |
| Spread of the true gaps around the line (Elo) | 25.8 | 25.8 |
| Spread of the career worths (Elo) | 24.7 | 24.5 |
| Typical error of a career worth (Elo) | 23.0 | 23.0 |
| Typical error of a career gap (Elo) | 63.5 | 63.5 |
| Tiers 1 to 5 among the listed | 7, 9, 17, 27, 40 | 6, 10, 17, 27, 40 |
| Expected members of the best 100 among the listed | 63.1 | 62.9 |
| Half-career check, 988 player halves: the games alone | 0.209 | 0.210 |
| Half-career check: the box score alone | 0.311 | 0.310 |
| Half-career check: the career worth | 0.325 | 0.326 |
| Half-career check: spread of the misses over their errors | 1.011 | 1.010 |

Between the two runs 243 of the 505 ranked players change place, most by one or two places. The only change of tier is Jordan's. The largest change of another player's worth is 0.74 Elo (Nikola Jokic). The first ten of 2.1 are Stephen Curry (143 ± 21), Nikola Jokic (143 ± 24), LeBron James (134 ± 20), Joel Embiid (122 ± 20), Magic Johnson (122 ± 21), Larry Bird (122 ± 22), Michael Jordan (115 ± 22), Giannis Antetokounmpo (111 ± 22), Kawhi Leonard (106 ± 20) and Luka Doncic (109 ± 23).
