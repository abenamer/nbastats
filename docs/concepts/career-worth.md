---
type: Model
title: Career worth and the Top 100
description: How the application ranks players over their careers, starting each player from what the box score of his career says and letting the gap between his teams' adjusted margins in the games he played and the games he missed move the number, each weighted by its precision, then draws the ranking many times and shows the first 100 in tiers on a page that is computed again after each data pull.
tags: [players, worth, career, ranking, tiers, elo, box-score]
generated: { by: "human:abenamer", at: "2026-11-11T17:35:42Z" }
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
  - id: list
    resource: /src/domain/declared_absences.rs
    title: The list of declared absences
  - id: declared
    resource: /docs/decisions/0021-declared-absences-for-long-injuries-and-retirements.md
    title: ADR 0021, the declared absences
  - id: first
    resource: /docs/decisions/0020-a-declared-absence-for-michael-jordans-retirement.md
    title: ADR 0020, the first declared absence
---

# Purpose

The Top 100 page shows the 100 players since 1976-77 with the largest career worth, in tiers. The career worth has two sources, and neither is enough alone.

- The **games** say what happened: by how much his teams' results changed between the games he played and the games he missed. This is the question the [player worth](/concepts/player-worth.md) asks of one season. The number is noisy, and a player who never sat has none.
- The **box score** says what his statistics are worth to a team ([Box-score worth](/concepts/box-score-worth.md)). The number is steady, and it does not see everything a player does.

The career worth starts each player from his box score and lets his games move the number, each weighted by how sure it is. It changes no rating, forecast or record. [ADR 0019](/decisions/0019-career-worth-in-tiers-from-games-with-and-without.md) records the decision, the first version that was rejected, and the runs. [ADR 0020](/decisions/0020-a-declared-absence-for-michael-jordans-retirement.md) adds the first declared absence, for one player, and [ADR 0021](/decisions/0021-declared-absences-for-long-injuries-and-retirements.md) declares each long injury or retirement that meets one criterion.

# Inputs

The regular-season games from 1976-77 with a result and with a box score for the team, the pre-game Elo of each opponent, and the box-score lines that say who played ([Player data](/concepts/player-data.md)). A line with minutes, or with statistics and no minutes, is a game played, as in the season worth. The season box worths come from the box-score table of each season from 1976-77; a season without Box Plus/Minus (1981-82 on the maintainer's store) is not read.

# Measure

1. **Stint.** One player with one team in one season, with at least 5 games played and 24 minutes a game. A stint runs from his first game for the team to his last. The team's games before the first and after the last are not absences. A [declared absence](#declared-absences) adds games to a stint by hand.
2. **Adjusted margin.** The team's margin, plus the opponent's pre-game Elo above 1500, less a home advantage of 100 Elo (none on a neutral court), at 28 Elo a point. The player's own team's rating is not used.
3. **Career gap.** Inside a stint, the gap is the mean adjusted margin of the games he played less that of the games he missed. The career gap pools the gaps, each weighted by its effective number of games (played times missed over their sum), times 28, in Elo. The error is 28 times the square root of the pooled variance of the margins inside the stints over the sum of the weights. The games of a declared season that are read inside a stint of another season count as fewer games; see [Declared absences](#declared-absences). A season that carries more than a third of the weight is the dominant season. A player with no game missed inside a stint has no career gap.
4. **Career box worth.** The box-score worth of each of his seasons, weighted by the minutes.
5. **The line.** A weighted line through every ranked player with a career gap: the career gap against the career box worth, with an intercept. A player's weight is one over the sum of his gap's squared error and the squared spread of the true gaps around the line; the spread is found with the line. The line needs 30 players. Without it, no player is ranked.
6. **Starting value.** The line's value at the player's career box worth. Its error holds the spread around the line and the error of the line at that box worth, and is never less than 5 Elo.
7. **Career worth.** The starting value and the career gap together, each weighted by its precision. A player without a career gap keeps the starting value.
8. **Who is ranked.** A player with 400 games as a regular and a career box worth. A shorter career is counted only.
9. **Draws.** 20,000 draws from a fixed seed. Each draw adds a normal error to every career worth and ranks the players. The chances are the shares of the draws that put a player in the best 10, 25, 50 and 100. The expected rank is his mean place.
10. **Tier.** The smallest group the player is more likely than not to belong to: tier 1 the best 10, tier 2 the best 25, tier 3 the best 50, tier 4 the best 100, tier 5 none of them. The players are ordered by tier, then by expected rank, then by career worth. The list is the first 100.
11. **Half-career check.** Each estimate is built on half of a player's seasons (the odd years or the even years: the gap, the box worth and the line of that half alone) and compared with the career gap of the other half. Three estimates are scored: the gap alone, shrunk toward the mean by its error; the starting value alone; and the career worth. A score is the correlation, the miss beyond what the errors of the target explain, and the spread of the misses over their stated errors, which is 1 when the errors are honest.

# Declared absences

The store has no rosters of past seasons. Thus the stint rule cannot see a season a player sat out in whole: he has no game, and he has no stint. It also does not read the games before his first game of a season or after his last. A declared absence says by hand what the store cannot. It names a player, a team, a season and one of the player's stints with that team. Every game the team played in the season is then read as inside that stint. The games he did not play become games he missed.

**The criterion.** A season is declared when all of these are true:

- one injury, one illness, or one retirement that he came back from kept him out of at least half of the games the team played in that season;
- his last game before the outage and his first game after it were for that team;
- the team did not trade, waive or release him during the outage;
- he has a stint with that team in that season, or in the season before, or in the season after.

A suspension, a holdout, a season in another league, a coach's decision and an outage that ended a career are not declared. An outage of less than half of the team's games is not declared.

**The stint that receives the games.** His stint of the same season, when he has one. If not, his stint of the season before. If not, his stint of the season after.

**A season read inside a stint of another season counts as fewer games.** The two sides are then two teams of two seasons, and a team changes between two seasons with the player present in both. The measurement in ADR 0021 gives 3.1 points a game for that change. The `n` games of the other season count as `1 / (1/n + 3.1² / v)` games, where `v` is the pooled variance of one game inside the player's stints. A whole season of 82 games counts as about 14 games. If he played some games of that season without a stint there, those games are on no side.

**The list.** `DECLARED_ABSENCES` in `domain::declared_absences` holds 136 seasons of 107 players. Seven are retirements: Michael Jordan (1993-94 and 1994-95), Magic Johnson (1991-92, 1994-95 and 1995-96), Kevin Johnson (1998-99) and Robert Reid (1982-83). The others are injuries and illnesses, for example Klay Thompson (2019-20 and 2020-21), Larry Bird (1988-89), Kawhi Leonard (2021-22) and Grant Hill (four seasons with Orlando). [ADR 0021](/decisions/0021-declared-absences-for-long-injuries-and-retirements.md) gives each row with its cause, the candidates that were left out, and the runs. The list was made by hand and checked against the public record, most rows against one source. A player who meets the criterion and is not in the list is read by the stint rule alone.

The run of 2026-11-11 on the maintainer's store gave these numbers. The rows add 8,661 games missed, 4,796 of them of another season. Klay Thompson moves from 112th to 53rd, onto the list and into tier 4: his career gap rests on 199 games missed, not 63. Magic Johnson moves from 5th to 4th. Larry Bird moves from tier 1 to tier 2. Michael Jordan stays 7th, in tier 2, with a career worth of 118 ± 24 Elo. A row can lower a worth: Chris Webber falls from 162nd to 282nd, because Sacramento played well in the 58 games he missed in 2003-04.

The model version with the list is `career-gap-box-2.2`.

# Reading the page

The cards say how many players are ranked, the typical error against the spread of the career worths, how many of the 100 listed players are expected to be in the true best 100, the line from the box score to the games, and the half-career check. The run of 2026-11-11 gave 505 ranked players, a typical error of 24 Elo against a spread of 25, about 63 expected members, a line of 0.48 Elo of gap for each Elo of box worth, and a half-career correlation of 0.34 for the career worth against 0.23 for the games alone.

A tier is a statement of chance. A tier 4 player is more likely than not one of the best 100 of the ranked players; a tier 5 player on the list is there with less than an even chance. The **Worth** column is the career worth with its error, and the ranking draws from it. The **Box score** column is the starting value. The **Games** column is the career gap, and is empty for a player with no game missed inside a stint. The **Missed** column counts the team games he missed inside his stints, with the games of his declared absences. The **Note** column names the declared seasons of a player with their cause, says how many of the games missed are of another season and count for less, says when a player has no career gap, and when one season carries more than a third of the weight of the gap. **Find a player** shows where any ranked player stands, on the list or below it. **Active players only** keeps the [active players](/terminology.md), on the list and below it: the players the last roster refresh listed on a team. A place and a tier stay those among every ranked player. The address `/top100?active=1` opens the page with the filter on. On 2026-11-11, on the maintainer's store, 89 of the 505 ranked players were active, 30 of them on the list. A player who is not found has fewer than 400 games as a regular from 1976-77.

# Limits

- The list leans on the box score where a player missed few games. The box score undervalues defence and what does not reach a statistic.
- An absence is not an experiment. A player misses games when he is hurt, and a team often rests other players at the same time. The gap then reads more than his absence.
- The absences of team-mates are not separated. Two players who sit together share one gap.
- The gap is against the players who replaced him on that team. A deep team shows less for the same player.
- An absence at the start or the end of a stint is not read, unless it is declared. A declaration needs half of the team's games, so a season a player ended in March adds no absence, and neither does a season he started a month late.
- A season a player sat out in whole is not read, except by a declared absence. The list is made by hand, and it is not complete. A season with a team that the store has no box scores for cannot be declared.
- A team often changes itself after a long injury to a leading player: it trades players, rests others, or plays for a draft pick. The gap of a declared season then reads more than the absence. Damian Lillard moves from 23rd to 12th on the games Portland played after his last game of 2021-22.
- A declared season that is read inside a stint of another season compares two teams. It counts as fewer games for that reason. The size of the change between two seasons is measured with the player present, and a team that loses a leading player for a season can change more.
- Each declared row is a judgement about history that the store cannot check.
- The line gives one slope to every player. A player whose box score reads him better or worse than most is moved only by his own games.
- The seasons before 1976-77 and the playoffs are not read. A career that started earlier is read from 1976-77.
- The rank is a place among the ranked players only.
- The order by tier first can place a player above another with a better expected rank.
- The last season is the last season as a regular. An active player who has not yet 5 games at 24 minutes in the current season shows an earlier one.
- The active flag is the source's roster at the last refresh. A player without a team on that day is not active.
- The first read after an import or a player sync takes some seconds: the query, the box-score table of each season, the draws and the half-career check.

# Where it shows

The **Top 100** page (`/top100`). `GET /api/players/career-worth` serves the report; see the [API reference](/reference/api.md). The command `nba-elo-simulator career-worth` prints the same report as Markdown, with `--min-games`, `--draws`, `--seed` and `--listed` for other settings and `--json <path>` for the rows. The page and the API use the default settings only.
