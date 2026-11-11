---
type: Model
title: Career worth and the Top 100
description: How the application ranks players over their careers, starting each player from what the box score of his career says and letting the gap between his teams' adjusted margins in the games he played and the games he missed move the number, each weighted by its precision, then draws the ranking many times and shows the first 100 in tiers on a page that is computed again after each data pull.
tags: [players, worth, career, ranking, tiers, elo, box-score]
generated: { by: "human:abenamer", at: "2026-11-11T13:38:00Z" }
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
  - id: worth
    resource: /docs/concepts/player-worth.md
    title: Player worth and availability
  - id: box
    resource: /docs/concepts/box-score-worth.md
    title: Box-score worth
  - id: declared
    resource: /docs/decisions/0020-a-declared-absence-for-michael-jordans-retirement.md
    title: ADR 0020, the declared absence
---

# Purpose

The Top 100 page shows the 100 players since 1976-77 with the largest career worth, in tiers. The career worth has two sources, and neither is enough alone.

- The **games** say what happened: by how much his teams' results changed between the games he played and the games he missed. This is the question the [player worth](/concepts/player-worth.md) asks of one season. The number is noisy, and a player who never sat has none.
- The **box score** says what his statistics are worth to a team ([Box-score worth](/concepts/box-score-worth.md)). The number is steady, and it does not see everything a player does.

The career worth starts each player from his box score and lets his games move the number, each weighted by how sure it is. It changes no rating, forecast or record. [ADR 0019](/decisions/0019-career-worth-in-tiers-from-games-with-and-without.md) records the decision, the first version that was rejected, and the runs. [ADR 0020](/decisions/0020-a-declared-absence-for-michael-jordans-retirement.md) adds one declared absence, for one player.

# Inputs

The regular-season games from 1976-77 with a result and with a box score for the team, the pre-game Elo of each opponent, and the box-score lines that say who played ([Player data](/concepts/player-data.md)). A line with minutes, or with statistics and no minutes, is a game played, as in the season worth. The season box worths come from the box-score table of each season from 1976-77; a season without Box Plus/Minus (1981-82 on the maintainer's store) is not read.

# Measure

1. **Stint.** One player with one team in one season, with at least 5 games played and 24 minutes a game. A stint runs from his first game for the team to his last. The team's games before the first and after the last are not absences. A [declared absence](#the-declared-absence) adds games to a stint by hand.
2. **Adjusted margin.** The team's margin, plus the opponent's pre-game Elo above 1500, less a home advantage of 100 Elo (none on a neutral court), at 28 Elo a point. The player's own team's rating is not used.
3. **Career gap.** Inside a stint, the gap is the mean adjusted margin of the games he played less that of the games he missed. The career gap pools the gaps, each weighted by its effective number of games (played times missed over their sum), times 28, in Elo. The error is 28 times the square root of the pooled variance of the margins inside the stints over the sum of the weights. A season that carries more than a third of the weight is the dominant season. A player with no game missed inside a stint has no career gap.
4. **Career box worth.** The box-score worth of each of his seasons, weighted by the minutes.
5. **The line.** A weighted line through every ranked player with a career gap: the career gap against the career box worth, with an intercept. A player's weight is one over the sum of his gap's squared error and the squared spread of the true gaps around the line; the spread is found with the line. The line needs 30 players. Without it, no player is ranked.
6. **Starting value.** The line's value at the player's career box worth. Its error holds the spread around the line and the error of the line at that box worth, and is never less than 5 Elo.
7. **Career worth.** The starting value and the career gap together, each weighted by its precision. A player without a career gap keeps the starting value.
8. **Who is ranked.** A player with 400 games as a regular and a career box worth. A shorter career is counted only.
9. **Draws.** 20,000 draws from a fixed seed. Each draw adds a normal error to every career worth and ranks the players. The chances are the shares of the draws that put a player in the best 10, 25, 50 and 100. The expected rank is his mean place.
10. **Tier.** The smallest group the player is more likely than not to belong to: tier 1 the best 10, tier 2 the best 25, tier 3 the best 50, tier 4 the best 100, tier 5 none of them. The players are ordered by tier, then by expected rank, then by career worth. The list is the first 100.
11. **Half-career check.** Each estimate is built on half of a player's seasons (the odd years or the even years: the gap, the box worth and the line of that half alone) and compared with the career gap of the other half. Three estimates are scored: the gap alone, shrunk toward the mean by its error; the starting value alone; and the career worth. A score is the correlation, the miss beyond what the errors of the target explain, and the spread of the misses over their stated errors, which is 1 when the errors are honest.

# The declared absence

The store has no rosters of past seasons. Thus the stint rule cannot see a season a player sat out in whole: he has no game, and he has no stint. A declared absence says by hand what the store cannot. It names a player, a team, a season and one of the player's stints with that team. Every game the team played in the season is then read as inside that stint. The games he did not play become games he missed.

The list has one player. Michael Jordan left Chicago on 6 October 1993 and played again on 19 March 1995.

- The 82 games Chicago played in 1993-94 are read as games he missed in his stint of 1992-93.
- The 65 games Chicago played in 1994-95 before his return are read as games he missed in his stint of 1994-95, against the 17 games he played.

No other player has a declaration. This is the maintainer's choice, not a rule that the data applies, and [ADR 0020](/decisions/0020-a-declared-absence-for-michael-jordans-retirement.md) records it with the runs. The run of 2026-11-11 on the maintainer's store gave these numbers. With the declaration his career gap rests on 232 games missed, not 85, and is 88 ± 35 Elo, from 85 ± 53. His career worth goes down, from 123 ± 25 Elo to 115 ± 22. Chicago stayed a good team without him, so the gap hardly moves, and a firmer gap pulls the worth further below his starting value of 132. He stays in 7th place and moves from tier 1 to tier 2.

The model version with the declaration is `career-gap-box-2.1`.

# Reading the page

The cards say how many players are ranked, the typical error against the spread of the career worths, how many of the 100 listed players are expected to be in the true best 100, the line from the box score to the games, and the half-career check. The run of 2026-11-11 gave 505 ranked players, a typical error of 23 Elo against a spread of 25, about 63 expected members, a line of 0.48 Elo of gap for each Elo of box worth, and a half-career correlation of 0.33 for the career worth against 0.21 for the games alone.

A tier is a statement of chance. A tier 4 player is more likely than not one of the best 100 of the ranked players; a tier 5 player on the list is there with less than an even chance. The **Worth** column is the career worth with its error, and the ranking draws from it. The **Box score** column is the starting value. The **Games** column is the career gap, and is empty for a player with no game missed inside a stint. The **Note** column says when a row has a declared absence, when a player has no career gap, and when one season carries more than a third of the weight of the gap. **Find a player** shows where any ranked player stands, on the list or below it. **Active players only** keeps the [active players](/terminology.md), on the list and below it: the players the last roster refresh listed on a team. A place and a tier stay those among every ranked player. The address `/top100?active=1` opens the page with the filter on. On 2026-11-11, on the maintainer's store, 89 of the 505 ranked players were active, 30 of them on the list. A player who is not found has fewer than 400 games as a regular from 1976-77.

# Limits

- The list leans on the box score where a player missed few games. The box score undervalues defence and what does not reach a statistic.
- An absence is not an experiment. A player misses games when he is hurt, and a team often rests other players at the same time. The gap then reads more than his absence.
- The absences of team-mates are not separated. Two players who sit together share one gap.
- The gap is against the players who replaced him on that team. A deep team shows less for the same player.
- An absence at the start or the end of a stint is not read. A season a player ended in March adds no absence, and neither does a season he started late.
- A season a player sat out in whole is not read, except by a declared absence. One player has one, so he is measured with more evidence than his peers. A player who missed a whole season with an injury, or who retired and came back, is read by the stint rule alone. A comparison of Michael Jordan with such a player is not like for like.
- A declared absence can compare two seasons. The games Chicago played in 1993-94 are read against the games Jordan played in 1992-93, and the team around him was not the same in the two seasons.
- The line gives one slope to every player. A player whose box score reads him better or worse than most is moved only by his own games.
- The seasons before 1976-77 and the playoffs are not read. A career that started earlier is read from 1976-77.
- The rank is a place among the ranked players only.
- The order by tier first can place a player above another with a better expected rank.
- The last season is the last season as a regular. An active player who has not yet 5 games at 24 minutes in the current season shows an earlier one.
- The active flag is the source's roster at the last refresh. A player without a team on that day is not active.
- The first read after an import or a player sync takes some seconds: the query, the box-score table of each season, the draws and the half-career check.

# Where it shows

The **Top 100** page (`/top100`). `GET /api/players/career-worth` serves the report; see the [API reference](/reference/api.md). The command `nba-elo-simulator career-worth` prints the same report as Markdown, with `--min-games`, `--draws`, `--seed` and `--listed` for other settings and `--json <path>` for the rows. The page and the API use the default settings only.
