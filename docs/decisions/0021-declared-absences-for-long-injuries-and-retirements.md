---
type: Architecture Decision
title: Declared absences for long injuries and retirements
description: Why the career worth now reads 136 seasons of 107 players as declared absences (a season in which one injury, illness or retirement kept a player out of at least half of his team's games), the criterion for a row, why a season read against a stint of another season counts as fewer games, what the runs say the list changes, and which absences stay unread.
tags: [decision, players, worth, career, ranking, tiers, exception, injuries]
generated: { by: "human:abenamer", at: "2026-11-11T17:34:45Z" }
sources:
  - id: list
    resource: /src/domain/declared_absences.rs
    title: The list of declared absences, with the criterion
  - id: code
    resource: /src/domain/career.rs
    title: Career worth module, the games of another season
  - id: query
    resource: /src/adapters/postgres/player_store.rs
    title: The career cells query
  - id: use-case
    resource: /src/application/career.rs
    title: Career report use case
  - id: before
    resource: /docs/decisions/0020-a-declared-absence-for-michael-jordans-retirement.md
    title: ADR 0020, the declaration for one player that this decision makes general
  - id: model
    resource: /docs/decisions/0019-career-worth-in-tiers-from-games-with-and-without.md
    title: ADR 0019, the model
---

# ADR 0021 — Declared absences for long injuries and retirements

## Context

[ADR 0019](/decisions/0019-career-worth-in-tiers-from-games-with-and-without.md) reads an absence as a team game between a player's first game and his last game of a stint. [ADR 0020](/decisions/0020-a-declared-absence-for-michael-jordans-retirement.md) added a declared absence for one player, Michael Jordan, and said what that left open:

* He was measured with more evidence than his peers. Klay Thompson missed two seasons with injuries. Magic Johnson retired and came back. Both were still read by the window alone.
* A rule from the data could not close the gap. The store has no rosters of past seasons, and cannot tell a player who sat out from a player in another league, a player who held out, or a player on no team.

The maintainer then asked for any significant injury or retirement outage to be added.

A list made by hand can do what a rule from the data cannot. Each row is a statement about history, and each row can be checked against a public record.

The first run with such a list showed a second problem. ADR 0020 already named it: a season out in whole is read against the games of another season. With one row for one player the effect was small. With 65 such rows it is not. The run with plain rows put Klay Thompson 11th and made the half-career check of the career worth worse (0.318, from 0.326).

## Decision

### The criterion

A season of a player with a team is declared when all of these are true.

1. **One outage, of a kind he came back from.** One injury, one illness, or one retirement that he came back from kept him out of at least half of the games the team played in that season. Two short injuries do not add up.
2. **The same team before and after.** His last game before the outage and his first game after it were for that team.
3. **The team kept him.** The team did not trade, waive or release him during the outage.
4. **A stint receives the games.** The player has a stint with that team in that season, or in the season before, or in the season after.

These absences are not declared:

* a suspension, a holdout or a contract dispute;
* a season in another league;
* a coach's decision, and an absence that is not an injury, an illness or a retirement;
* an outage during which he was traded, waived or released, or his contract was voided;
* an outage that ended his career, or that he came back from with another team;
* an outage of less than half of the team's games. The window already reads such an absence when it lies between two of his games.

The store has box scores only for the teams it knows by their present codes. A season with a team whose code is no longer used (for example Seattle, New Jersey or Vancouver) cannot be read, and has no row.

### The stint that receives the games

* **His stint of the same season**, when he has one (71 rows). The games he played stay games played. The other games of the team become games he missed. This reads the start and the end of the season, which the window leaves out.
* **His stint of the season before**, when he has no stint in the season of the outage (47 rows).
* **His stint of the season after**, when he has neither (18 rows).

### A season read against another season counts as fewer games

Inside a normal stint the two sides are of the same season: the same roster, coach and league. A declared season that is read inside a stint of another season compares two teams. A team changes between two seasons, with the player present in both.

* **The size of the change.** The measurement below gives a standard deviation of 3.1 points a game between a team's adjusted margin in two consecutive seasons of the same player, after the sampling error is taken out. `BETWEEN_SEASON_POINTS` holds the value.
* **The count.** The `n` games of the other season count as `1 / (1/n + 3.1² / v)` games, where `v` is the pooled variance of one game inside that player's stints. For a usual `v` (a standard deviation near 12.5 points), a season of 82 games counts as about 14 games. No number of games of another season can count as more than about 16.
* **The mean.** The games missed in the stint's own season and the games of the other season give one mean without him, each weighted by its count.
* **The games he played in the other season.** If he played some games of the other season without a stint there (fewer than 5 games, or fewer than 24 minutes a game), those games are on no side.
* **Two other seasons in one stint are pooled as one.** Lafayette Lever's stint of 1991-92 receives 1990-91 and 1992-93. The 160 games count as about 15, not as two times 14.

The report gives the games of another season as `gap.missed_other_season`. They are part of `gap.missed`.

### The list

`DECLARED_ABSENCES` in `domain::declared_absences` is the whole list: 136 rows for 107 players. Seven rows are retirements (Michael Jordan, Magic Johnson, Kevin Johnson and Robert Reid). The others are injuries and illnesses. [The rows](#the-rows) below gives each row with its cause.

* Michael Jordan keeps the two rows of ADR 0020. His 1993-94 row is read inside his stint of 1992-93, so it now counts for less.
* A row names the player by the source's key and the team by its code, as in ADR 0020. A row whose player, team or stint is not in the store adds nothing.
* A row carries the player's name and the cause. The report shows both under `rule.declared`.

### The page and the version

* The Note column of the Top 100 page shows the declared seasons of a player with their cause, and says how many of the games missed are of another season.
* **How it is computed** on the page states the criterion, the weight of a season of another season, and what is not declared.
* The command `nba-elo-simulator career-worth` prints one line for each row.
* The model version is `career-gap-box-2.2`. The rest of the model of ADR 0019 is unchanged.

## Consequences

* **The comparison of ADR 0020 is like for like again.** Each player who meets the criterion is read in the same way as Michael Jordan.
* **Klay Thompson is on the list.** He moves from 112th to 53rd and from tier 5 to tier 4. His career gap now rests on 199 games missed, not 63, and is 162 ± 45 Elo, from 112 ± 51. His chance of the best 100 goes from 29% to 56%.
* **Magic Johnson moves from 5th to 4th.** The Lakers of 1991-92 without him were a much weaker team. His career gap is 161 ± 31 Elo, from 145 ± 34.
* **Larry Bird moves from tier 1 to tier 2** and stays 6th. The 76 games Boston played without him in 1988-89 went better than his other absences. His career gap is 127 ± 38 Elo, from 146 ± 40, and his chance of the best 10 is 47%.
* **Michael Jordan stays 7th, in tier 2.** His career worth is 118 ± 24 Elo, from 115 ± 22. The 82 games of 1993-94 now count as about 14 games, so the error of his gap goes back up, from 35 to 43 Elo, and the worth moves back toward his starting value.
* **A row can lower a worth.** Chris Webber falls from 162nd to 282nd: Sacramento played well in the 58 games he missed in 2003-04. Chuck Person rises the most, by 22 Elo.
* **The list changes by three players.** Klay Thompson, Ron Harper and Eric Bledsoe come onto the list. Chauncey Billups, Derrick White and Michael Redd leave it.
* **Players without a row move a little.** The line from the box worth to the gap moves, and each starting value with it. The largest change of the worth of a player without a row is 2.6 Elo. Thirteen players change tier. Seven of them have no row: the worths of the players near a boundary moved by a little, and the boundaries with them.
* **The half-career check is a little better.** The correlation of the career worth with the games of the other half is 0.336, from 0.326. The spread of the misses over their errors stays near 1 (1.007).

The limits that this decision adds or keeps:

* **A team often changes itself after a long injury.** It trades players, rests other players, or plays for a draft pick. The gap then reads more than the absence. Damian Lillard moves from 23rd to 12th. His row adds the 46 games Portland played after his last game of 2021-22. Portland traded three regular players in that season and rested others. A row of the same season carries its full weight, and the model cannot see this.
* **Team-mates who are out at the same time share the gap.** Golden State played most of 2019-20 without Stephen Curry also. Klay Thompson's row for that season reads that team.
* **The size of the change between seasons is measured with the player present.** A team that loses a leading player for a season can change more than that.
* **Each row is a judgement about history.** Most rows rest on one public source. The store cannot check a row. A wrong row changes one player's gap.
* **The list is not complete.** It was made in one search on one day. A player who meets the criterion and is not in the list is read by the window alone. An absence of less than half of a season outside the window stays unread.
* **A row at the edge of the criterion.** Six rows are at half of the team's games and no more (Blake Griffin 2019-20, Brad Daugherty 1989-90, Grant Hill 2002-03, Lafayette Lever 1991-92, Rick Fox 2003-04 and Ron Harper 1990-91). Chris Webber's 58 games out in 2003-04 end with a suspension of 8 games. Monta Ellis in 2008-09 and Kevin Willis in 1988-89 were suspended by their teams for some games during the injury.
* Nothing here changes a rating, a forecast or the market record.

## Alternatives

* **The rows without the smaller count.** Each game of another season counts as one game. This was run (the column "Plain rows" below). It put Klay Thompson 11th, in tier 2, with a gap of 213 ± 38 Elo, and Robert Reid 22nd. The half-career correlation of the career worth fell to 0.318, and the spread of the misses over their errors rose to 1.029: the errors were too small. Rejected.
* **A rule from the data**: a season without a game between two seasons with the same team. It needs no list. It cannot tell an injury from a season in another league (Joe Barry Carroll in 1984-85, Ersan Ilyasova from 2007 to 2009, Wilson Chandler in 2011-12) or from a holdout. It also does not see the 71 seasons in which he played some games. Not run.
* **Rows of the same season only.** No comparison between seasons is then needed. The seasons out in whole stay unread: Klay Thompson's two seasons, Magic Johnson's 1991-92 and Michael Jordan's 1993-94, which are the cases the request named. Not run.
* **A lower floor**, for example a quarter of the team's games. The list would be several times longer, with more judgement in each row, and the window already reads most of those absences. Not run.
* **Suspensions and holdouts.** A team does not plan for an injury. It can plan for a holdout, and a suspension is often short. The maintainer asked for injuries and retirements. Not declared (Gilbert Arenas in 2009-10, Metta World Peace in 2004-05).
* **The one row of ADR 0020 and no more** keeps the unlike comparison that ADR 0020 named as its cost.

## Runs

All runs are of 2026-11-11, on the maintainer's store (the Docker Compose database of that day).

### The change of a team between two seasons

A query on the store took each pair of stints of the same player with the same team in two consecutive seasons, 4,747 pairs. For each pair it compared the team's mean adjusted margin in the games he played in the two seasons.

| Measure | Points a game | Elo |
|---|---|---|
| Mean difference | −0.05 | |
| Root mean square of the difference | 3.90 | 109 |
| The part that sampling explains | 2.36 | 66 |
| Standard deviation between the two seasons | 3.11 | 87 |
| The same, stints of at least 20 games | 3.04 | 85 |
| The same, stints of at least 41 games | 2.94 | 82 |
| The same, stints of at least 60 games | 2.85 | 80 |
| Two seasons apart | 4.24 | 119 |

Each row of the list is read inside a stint of the same season or of the next one, so the value for consecutive seasons applies.

### The whole report

The three runs are the command `nba-elo-simulator career-worth --json` in a release build, 20,000 draws, seed 1977, the first 100 listed. "Plain rows" is the 136 rows with each game of another season counted as one game. The runs of the plain rows and of `career-gap-box-2.2` were made before the change was on a page.

| Measure | 2.1 | Plain rows | 2.2 |
|---|---|---|---|
| Declared rows | 2 | 136 | 136 |
| Players with a stint | 1,543 | 1,543 | 1,543 |
| Ranked | 505 | 505 | 505 |
| Ranked without a career gap | 2 | 2 | 2 |
| Games missed, over the ranked players | 33,575 | 42,089 | 42,089 |
| Of them, games of another season | | | 4,796 |
| The line, over 503 players | 10.9 + 0.485 (± 0.052) × box worth | 13.5 + 0.470 (± 0.054) × box worth | 12.0 + 0.481 (± 0.052) × box worth |
| Spread of the true gaps around the line (Elo) | 25.8 | 32.0 | 26.8 |
| Spread of the career worths (Elo) | 24.5 | 27.4 | 24.9 |
| Typical error of a career worth (Elo) | 23.0 | 26.7 | 23.6 |
| Typical error of a career gap (Elo) | 63.5 | 60.9 | 61.6 |
| Tiers 1 to 5 among the listed | 6, 10, 17, 27, 40 | 5, 8, 21, 27, 39 | 5, 10, 16, 28, 41 |
| Expected members of the best 100 among the listed | 62.9 | 62.6 | 63.2 |
| Half-career check, player halves | 988 | 989 | 989 |
| Half-career check: the games alone | 0.210 | 0.231 | 0.233 |
| Half-career check: the box score alone | 0.310 | 0.302 | 0.312 |
| Half-career check: the career worth | 0.326 | 0.318 | 0.336 |
| Half-career check: spread of the misses over their errors | 1.010 | 1.029 | 1.007 |

Between 2.1 and 2.2, 452 of the 505 ranked players change place. The tier changes are: Damian Lillard 3 to 2; Brad Daugherty, Klay Thompson and Ron Harper 5 to 4; Larry Bird 1 to 2; Charles Barkley and Hakeem Olajuwon 2 to 3; Tim Duncan and Larry Nance 3 to 4; Tim Hardaway, Dirk Nowitzki, Bob Lanier and Andrei Kirilenko 4 to 5. The first twelve of 2.2 are Stephen Curry (145 ± 21), Nikola Jokic (143 ± 25), LeBron James (135 ± 20), Magic Johnson (132 ± 21), Joel Embiid (123 ± 21), Larry Bird (117 ± 23), Michael Jordan (118 ± 24), Giannis Antetokounmpo (111 ± 22), Kawhi Leonard (110 ± 20), Luka Doncic (108 ± 24), Paul George (105 ± 21) and Damian Lillard (104 ± 23).

### Some players

Worth and gap are in Elo with the standard error. The count in brackets is the games of another season.

| Player | Place 2.1 | Tier | Worth | Gap | Missed | Place 2.2 | Tier | Worth | Gap | Missed (of another season) |
|---|---|---|---|---|---|---|---|---|---|---|
| Stephen Curry | 1 | 1 | 143 ± 21 | 208 ± 33 | 228 | 1 | 1 | 145 ± 21 | 208 ± 33 | 228 (0) |
| Nikola Jokic | 2 | 1 | 143 ± 24 | 170 ± 48 | 69 | 2 | 1 | 143 ± 25 | 170 ± 48 | 69 (0) |
| LeBron James | 3 | 1 | 134 ± 20 | 147 ± 28 | 184 | 3 | 1 | 135 ± 20 | 147 ± 28 | 184 (0) |
| Magic Johnson | 5 | 1 | 122 ± 21 | 145 ± 34 | 112 | 4 | 1 | 132 ± 21 | 161 ± 31 | 318 (164) |
| Joel Embiid | 4 | 1 | 122 ± 20 | 147 ± 31 | 223 | 5 | 1 | 123 ± 21 | 147 ± 31 | 223 (0) |
| Larry Bird | 6 | 1 | 122 ± 22 | 146 ± 40 | 80 | 6 | 2 | 117 ± 23 | 127 ± 38 | 156 (0) |
| Michael Jordan | 7 | 2 | 115 ± 22 | 88 ± 35 | 232 | 7 | 2 | 118 ± 24 | 86 ± 43 | 232 (82) |
| Giannis Antetokounmpo | 8 | 2 | 111 ± 22 | 124 ± 37 | 124 | 8 | 2 | 111 ± 22 | 124 ± 37 | 124 (0) |
| Kawhi Leonard | 9 | 2 | 106 ± 20 | 123 ± 30 | 191 | 9 | 2 | 110 ± 20 | 129 ± 30 | 273 (82) |
| Luka Doncic | 10 | 2 | 109 ± 23 | 60 ± 45 | 97 | 10 | 2 | 108 ± 24 | 60 ± 45 | 97 (0) |
| Paul George | 12 | 2 | 102 ± 21 | 162 ± 36 | 167 | 11 | 2 | 105 ± 21 | 159 ± 34 | 243 (76) |
| Damian Lillard | 23 | 3 | 93 ± 23 | 116 ± 45 | 73 | 12 | 2 | 104 ± 23 | 148 ± 42 | 119 (0) |
| Kyrie Irving | 20 | 3 | 95 ± 20 | 114 ± 30 | 201 | 16 | 3 | 99 ± 20 | 119 ± 29 | 283 (82) |
| David Robinson | 39 | 4 | 78 ± 24 | -23 ± 55 | 35 | 36 | 4 | 82 ± 24 | 15 ± 51 | 111 (0) |
| Ron Harper | 107 | 5 | 53 ± 23 | 85 ± 50 | 53 | 45 | 4 | 75 ± 22 | 129 ± 37 | 148 (0) |
| Brad Daugherty | 84 | 5 | 59 ± 23 | 86 ± 54 | 35 | 49 | 4 | 71 ± 23 | 115 ± 43 | 76 (0) |
| Klay Thompson | 112 | 5 | 52 ± 23 | 112 ± 51 | 63 | 53 | 4 | 70 ± 23 | 162 ± 45 | 199 (136) |
| Eric Bledsoe | 129 | 5 | 50 ± 23 | 86 ± 55 | 59 | 66 | 5 | 65 ± 23 | 128 ± 45 | 109 (0) |
| Derrick Rose | 80 | 5 | 59 ± 21 | 85 ± 35 | 130 | 91 | 5 | 58 ± 21 | 76 ± 32 | 336 (82) |
| Grant Hill | 118 | 5 | 50 ± 21 | 41 ± 35 | 108 | 116 | 5 | 52 ± 20 | 45 ± 31 | 376 (160) |
| Robert Reid | 244 | 5 | 35 ± 23 | 71 ± 49 | 46 | 139 | 5 | 49 ± 23 | 115 ± 45 | 128 (82) |
| Chris Webber | 162 | 5 | 45 ± 21 | 5 ± 38 | 92 | 282 | 5 | 32 ± 21 | -22 ± 34 | 150 (0) |
| Chuck Person | 487 | 5 | 5 ± 24 | -117 ± 64 | 25 | 332 | 5 | 26 ± 24 | 28 ± 54 | 107 (82) |

### The rows on the store

A query on the store counted, for each row, the games it adds as missed.

* Each of the 136 rows finds its player, its team and its stint, and adds games. All 107 players are ranked.
* The 71 rows of the same season add 3,865 games. The 65 rows read inside another season add 4,796 games. The sum is 8,661, and 58 ranked players have games of another season.
* Five rows are in 1998-99, a season of 50 games. The smallest additions are LaPhonso Ellis in 1998-99 (30 games), Eric Gordon in 2020-21 (37), and Raef LaFrentz and Voshon Lenard in 1998-99 (38 each).
* In twelve rows read inside another season, he played some games of the declared season without a stint there. Those games are on no side: Rick Fox 2003-04 (38 games), Terry Porter 1994-95 (35), Bryant Stith 1997-98 (31), Mike Dunleavy 2015-16 (31), Wally Szczerbiak 2003-04 (28), Trevor Ariza 2007-08 (24), Jamal Crawford 2001-02 (23), Dorian Finney-Smith 2017-18 (21), Sean Elliott 1999-00 (19), Dillon Brooks 2018-19 (18), Raef LaFrentz 2003-04 (17) and Brook Lopez 2021-22 (13).

### The check of the candidates

A search of the store listed the seasons in which a player with a stint was out for a long run of his team's games, or for a whole season between two seasons with the same team. 137 of them, beside the two rows of ADR 0020, met the parts of the criterion that the store can show. Each was checked against one public source: Wikipedia for 87, the game logs of Basketball-Reference for 14, and news reports for the others (UPI, Sports Illustrated, NBA.com and others). 134 passed. Three did not:

| Candidate | Why it has no row |
|---|---|
| James Edwards, Phoenix, 1986-87 | The injury was not identified. |
| Michael Cooper, Los Angeles Lakers, 1978-79 | The absence was not confirmed as an injury absence. |
| Jeff Green, Boston, 2011-12 | Boston voided his contract after the diagnosis (criterion 3). |

Absences that were looked at and are outside the criterion:

| Absence | Why it has no row |
|---|---|
| Gilbert Arenas 2009-10, Metta World Peace 2004-05 | A suspension. |
| Joe Barry Carroll 1984-85, Ersan Ilyasova 2007-09, Wilson Chandler 2011-12 | A season in another league. |
| Miles Bridges 2022-23 | Not an injury, an illness or a retirement. |
| Ricky Rubio (Cleveland, 2021-22), Doc Rivers (New York, 1994-95), Mario Chalmers 2016-17, Spencer Dinwiddie 2020-21 | Traded or waived during the outage, or with a new team at the end of it. |
| Kevin Durant 2019-20, Kawhi Leonard 2017-18, Michael Jordan 1998-99 to 2000-01 | His next game was for another team. |
| Yao Ming 2010-11, Jason Richardson 2014-15, Al-Farouq Aminu (Orlando, 2019-20), Lindsey Hunter 2005-06, Steven Adams 2025-26, Magic Johnson 1992-93 and 1993-94, Kevin Johnson 1999-00 | No stint of that season or of a season next to it receives the games. |
| Klay Thompson 2021-22, Michael Redd 2008-09, Victor Oladipo 2018-19, Jamal Murray 2020-21 | Less than half of the team's games in that season. |

### The rows

The season out is the season whose team games are read. The games added are from the store of 2026-11-11.

| Player | Team | Season out | Stint that receives the games | Cause | Games added as missed |
|---|---|---|---|---|---|
| Adrian Dantley | UTA | 1982-83 | 1982-83 | Torn wrist ligaments | 58 |
| Al Horford | ATL | 2011-12 | 2011-12 | Torn pectoral muscle | 55 |
| Al Horford | ATL | 2013-14 | 2013-14 | Torn pectoral muscle | 53 |
| Amar'e Stoudemire | PHO | 2005-06 | 2004-05 | Knee surgery | 79 |
| Anfernee Hardaway | PHO | 2000-01 | 1999-00 | Knee surgeries | 78 |
| Anthony Peeler | LAL | 1993-94 | 1993-94 | Stress fracture of the leg | 52 |
| Antonio McDyess | DEN | 2001-02 | 2000-01 | Knee surgery | 72 |
| Bill Cartwright | NYK | 1984-85 | 1983-84 | Foot fractures | 82 |
| Bill Cartwright | NYK | 1985-86 | 1986-87 | Foot fractures | 80 |
| Blake Griffin | DET | 2019-20 | 2019-20 | Knee surgery | 43 |
| Brad Daugherty | CLE | 1989-90 | 1989-90 | Foot surgery | 41 |
| Brook Lopez | BRK | 2013-14 | 2013-14 | Broken foot | 56 |
| Brook Lopez | MIL | 2021-22 | 2020-21 | Back surgery | 69 |
| Bryant Stith | DEN | 1997-98 | 1996-97 | Foot surgery | 51 |
| Calvin Natt | DEN | 1986-87 | 1985-86 | Torn Achilles tendon | 81 |
| Carlos Boozer | UTA | 2005-06 | 2005-06 | Hamstring injury | 49 |
| Channing Frye | PHO | 2012-13 | 2011-12 | Enlarged heart | 82 |
| Chauncey Billups | LAC | 2011-12 | 2011-12 | Torn Achilles tendon | 44 |
| Chris Mills | GSW | 1999-00 | 1999-00 | Ankle surgery | 56 |
| Chris Mills | GSW | 2000-01 | 2000-01 | Ankle surgery | 65 |
| Chris Webber | SAC | 2003-04 | 2003-04 | Knee surgery | 58 |
| Chuck Person | SAS | 1996-97 | 1995-96 | Back surgery | 82 |
| Damian Lillard | POR | 2021-22 | 2021-22 | Abdominal surgery | 46 |
| Damon Stoudamire | MEM | 2005-06 | 2005-06 | Torn patellar tendon | 54 |
| Danilo Gallinari | DEN | 2013-14 | 2012-13 | Torn knee ligament | 82 |
| Danny Granger | IND | 2012-13 | 2011-12 | Knee injury | 76 |
| Danny Manning | LAC | 1988-89 | 1988-89 | Torn knee ligament | 56 |
| Danny Manning | PHO | 1995-96 | 1995-96 | Torn knee ligament | 42 |
| Darrell Griffith | UTA | 1985-86 | 1984-85 | Stress fracture of the foot | 82 |
| David Robinson | SAS | 1996-97 | 1996-97 | Broken foot, after a back injury | 76 |
| Dee Brown | BOS | 1991-92 | 1991-92 | Knee surgery | 50 |
| Dejounte Murray | SAS | 2018-19 | 2019-20 | Torn knee ligament | 82 |
| Dejounte Murray | NOP | 2025-26 | 2025-26 | Torn Achilles tendon | 63 |
| Dennis Scott | ORL | 1991-92 | 1991-92 | Knee surgery | 58 |
| Derek Fisher | LAL | 2000-01 | 2000-01 | Foot stress fracture | 62 |
| Derrick Rose | CHI | 2012-13 | 2011-12 | Torn knee ligament | 82 |
| Derrick Rose | CHI | 2013-14 | 2013-14 | Torn meniscus | 71 |
| Derrick Rose | NYK | 2021-22 | 2021-22 | Ankle surgery | 53 |
| Dillon Brooks | MEM | 2018-19 | 2017-18 | Toe surgery | 64 |
| Doc Rivers | NYK | 1993-94 | 1993-94 | Torn knee ligament | 63 |
| Dorian Finney-Smith | DAL | 2017-18 | 2018-19 | Knee tendinitis | 61 |
| Elton Brand | LAC | 2007-08 | 2007-08 | Torn Achilles tendon | 74 |
| Emeka Okafor | CHO | 2005-06 | 2005-06 | Ankle injury | 48 |
| Eric Bledsoe | PHO | 2015-16 | 2015-16 | Torn meniscus | 50 |
| Eric Gordon | HOU | 2020-21 | 2020-21 | Groin strain | 37 |
| Fred VanVleet | HOU | 2025-26 | 2024-25 | Torn knee ligament | 82 |
| Gilbert Arenas | WAS | 2008-09 | 2007-08 | Knee surgeries | 80 |
| Glen Rice | HOU | 2001-02 | 2001-02 | Torn patellar tendon | 62 |
| Gordon Hayward | BOS | 2017-18 | 2018-19 | Broken leg | 81 |
| Grant Hill | ORL | 2000-01 | 2001-02 | Ankle injury | 78 |
| Grant Hill | ORL | 2001-02 | 2001-02 | Ankle injury | 67 |
| Grant Hill | ORL | 2002-03 | 2002-03 | Ankle injury | 41 |
| Grant Hill | ORL | 2003-04 | 2002-03 | Ankle injury | 82 |
| Jamal Crawford | CHI | 2001-02 | 2002-03 | Torn knee ligament | 59 |
| Jamal Mashburn | DAL | 1995-96 | 1995-96 | Knee injury | 64 |
| Jamal Mashburn | NOP | 2003-04 | 2003-04 | Knee injury | 63 |
| Jamal Murray | DEN | 2021-22 | 2020-21 | Torn knee ligament | 80 |
| Jaren Jackson Jr. | MEM | 2020-21 | 2019-20 | Torn meniscus | 57 |
| Jason Richardson | PHI | 2012-13 | 2012-13 | Knee surgery | 42 |
| Jason Richardson | PHI | 2013-14 | 2012-13 | Knee surgery | 82 |
| Jayson Tatum | BOS | 2025-26 | 2025-26 | Torn Achilles tendon | 64 |
| Jerry Stackhouse | WAS | 2003-04 | 2003-04 | Knee surgery | 48 |
| Jrue Holiday | NOP | 2013-14 | 2013-14 | Stress fracture of the leg | 48 |
| Julius Randle | LAL | 2014-15 | 2015-16 | Broken leg | 81 |
| Jusuf Nurkic | POR | 2019-20 | 2019-20 | Broken leg | 66 |
| Kawhi Leonard | LAC | 2021-22 | 2020-21 | Torn knee ligament | 82 |
| Kendrick Perkins | BOS | 2010-11 | 2010-11 | Torn knee ligament | 70 |
| Kevin Johnson | PHO | 1998-99 | 1997-98 | Retirement | 50 |
| Kevin Love | MIN | 2012-13 | 2012-13 | Broken hand | 62 |
| Kevin Willis | ATL | 1988-89 | 1987-88 | Broken foot | 82 |
| Khris Middleton | MIL | 2016-17 | 2016-17 | Torn hamstring | 51 |
| Kiki Vandeweghe | NYK | 1989-90 | 1989-90 | Back injury | 60 |
| Klay Thompson | GSW | 2019-20 | 2018-19 | Torn knee ligament | 65 |
| Klay Thompson | GSW | 2020-21 | 2021-22 | Torn Achilles tendon | 71 |
| Kobe Bryant | LAL | 2013-14 | 2013-14 | Knee fracture, after a torn Achilles tendon | 76 |
| Kyle Lowry | MEM | 2006-07 | 2007-08 | Broken wrist | 72 |
| Kyrie Irving | DAL | 2025-26 | 2024-25 | Torn knee ligament | 82 |
| LaPhonso Ellis | DEN | 1994-95 | 1993-94 | Knee surgery | 76 |
| LaPhonso Ellis | ATL | 1998-99 | 1998-99 | Hernia surgery | 30 |
| Lafayette Lever | DAL | 1990-91 | 1991-92 | Knee injury | 78 |
| Lafayette Lever | DAL | 1991-92 | 1991-92 | Knee injury | 41 |
| Lafayette Lever | DAL | 1992-93 | 1991-92 | Knee injury | 82 |
| Lamar Odom | LAC | 2001-02 | 2001-02 | Wrist injury | 42 |
| Larry Bird | BOS | 1988-89 | 1988-89 | Surgery on both heels | 76 |
| Lou Williams | ATL | 2012-13 | 2012-13 | Torn knee ligament | 43 |
| Magic Johnson | LAL | 1991-92 | 1990-91 | Retirement | 82 |
| Magic Johnson | LAL | 1994-95 | 1995-96 | Retirement | 82 |
| Magic Johnson | LAL | 1995-96 | 1995-96 | Retirement | 42 |
| Marcus Smart | MEM | 2023-24 | 2023-24 | Finger injury | 45 |
| Mark Price | CLE | 1990-91 | 1990-91 | Torn knee ligament | 66 |
| Matt Harpring | UTA | 2003-04 | 2003-04 | Knee surgery | 50 |
| Michael Jordan | CHI | 1993-94 | 1992-93 | Retirement | 82 |
| Michael Jordan | CHI | 1994-95 | 1994-95 | Retirement | 65 |
| Michael Redd | MIL | 2009-10 | 2009-10 | Torn knee ligaments | 48 |
| Michael Redd | MIL | 2010-11 | 2009-10 | Torn knee ligaments | 72 |
| Mike Conley | MEM | 2017-18 | 2017-18 | Heel surgery | 69 |
| Mike Dunleavy | CHI | 2015-16 | 2014-15 | Back surgery | 51 |
| Monta Ellis | GSW | 2008-09 | 2008-09 | Torn ankle ligament | 50 |
| Moses Malone | MIL | 1992-93 | 1991-92 | Back surgery | 71 |
| Mychal Thompson | POR | 1979-80 | 1978-79 | Broken leg | 82 |
| Nene | DEN | 2005-06 | 2006-07 | Torn knee ligament | 81 |
| Nicolas Batum | POR | 2009-10 | 2009-10 | Shoulder surgery | 45 |
| Norm Nixon | LAC | 1986-87 | 1985-86 | Torn knee tendon | 82 |
| Norm Nixon | LAC | 1987-88 | 1988-89 | Torn Achilles tendon | 82 |
| Patrick Beverley | LAC | 2017-18 | 2017-18 | Knee surgery | 66 |
| Patrick Ewing | NYK | 1997-98 | 1997-98 | Wrist injury | 56 |
| Paul George | IND | 2014-15 | 2013-14 | Broken leg | 76 |
| Raef LaFrentz | DEN | 1998-99 | 1998-99 | Torn knee ligament | 38 |
| Raef LaFrentz | BOS | 2003-04 | 2004-05 | Knee surgery | 65 |
| Randy Foye | MIN | 2007-08 | 2007-08 | Stress reaction in the kneecap | 43 |
| Rick Fox | LAL | 2003-04 | 2002-03 | Foot surgery | 44 |
| Robert Covington | MIN | 2018-19 | 2018-19 | Knee bone bruise | 59 |
| Robert Reid | HOU | 1982-83 | 1981-82 | Retirement | 82 |
| Ron Harper | LAC | 1989-90 | 1989-90 | Torn knee ligament | 54 |
| Ron Harper | LAC | 1990-91 | 1990-91 | Torn knee ligament | 41 |
| Ryan Anderson | NOP | 2013-14 | 2013-14 | Herniated disc in the neck | 60 |
| Samuel Dalembert | PHI | 2002-03 | 2003-04 | Knee surgery | 82 |
| Sean Elliott | SAS | 1997-98 | 1997-98 | Quadriceps tendinitis | 42 |
| Sean Elliott | SAS | 1999-00 | 1998-99 | Kidney transplant | 63 |
| Terry Cummings | SAS | 1992-93 | 1991-92 | Torn knee ligament | 74 |
| Terry Porter | POR | 1994-95 | 1993-94 | Ankle injury | 47 |
| Theo Ratliff | ATL | 2001-02 | 2002-03 | Hip injury | 79 |
| Tim Hardaway | GSW | 1993-94 | 1992-93 | Torn knee ligament | 82 |
| Trevor Ariza | LAL | 2007-08 | 2008-09 | Broken foot | 58 |
| Udonis Haslem | MIA | 2010-11 | 2010-11 | Torn foot ligament | 69 |
| Victor Oladipo | IND | 2019-20 | 2019-20 | Ruptured quadriceps tendon | 48 |
| Victor Oladipo | MIA | 2021-22 | 2022-23 | Quadriceps tendon surgery | 73 |
| Voshon Lenard | MIA | 1998-99 | 1997-98 | Stress fracture of the leg | 38 |
| Voshon Lenard | DEN | 2004-05 | 2003-04 | Torn Achilles tendon | 79 |
| Wally Szczerbiak | MIN | 2003-04 | 2002-03 | Foot injury | 54 |
| Walter Davis | PHO | 1984-85 | 1984-85 | Torn knee ligaments | 57 |
| Wilson Chandler | DEN | 2015-16 | 2014-15 | Hip surgery | 82 |
| Yao Ming | HOU | 2009-10 | 2008-09 | Foot fracture | 82 |
| Zydrunas Ilgauskas | CLE | 1998-99 | 1998-99 | Foot injury | 45 |
| Zydrunas Ilgauskas | CLE | 1999-00 | 1998-99 | Broken foot | 82 |
| Zydrunas Ilgauskas | CLE | 2000-01 | 2000-01 | Foot injury | 58 |
