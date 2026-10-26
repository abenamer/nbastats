---
type: Model
title: Player worth and availability
description: How the application values a player in Elo points from the games his team played without him, and how the latest injury report becomes a rating penalty in the season forecast.
tags: [players, worth, injuries, forecast, elo]
generated: { by: "human:abenamer", at: "2026-10-26T08:00:00Z" }
sources:
  - id: code
    resource: /src/domain/worth.rs
    title: Worth module
  - id: engine
    resource: /src/domain/playoffs/season.rs
    title: Season engine
---

# Purpose

A rating knows which team won. It does not know who played. The Players page answers "what is this player worth to his team's rating?" from the games the team played without him. The Season Forecast page can then take the latest injury report off each team's rating for the games to come. The ratings themselves do not change: the worth is a reading of the stored games, and the penalty is a parameter of a forecast.

# Absences

A player is absent from a game when he played for the team earlier and later in the season but not in that game. A box-score line with no minutes (a player who dressed and did not play) counts as an absence. Two lines for the same player and game count once. A player with fewer than five games played has no worth. A trade ends the player's run with one team and starts another. A season-long injury after the first games gives no absence, because nothing bounds it, and the model does not try to guess.

# Measure

1. Every played game has a residual: the actual margin minus the margin the pre-game ratings expected. The expectation uses 28 Elo points per point of margin and the ratings' own home bonus of 100. The 28 is FiveThirtyEight's rule for the ratings the application replays: a rating gap of 28 is a one-point spread, so the home bonus of 100 is about 3.5 points. It is one constant for every era, although a point of margin meant more in the low-scoring seasons. The residual holds everything the ratings did not know, the line-ups included.
2. The residuals of the games with an absence are regressed on the absences of both sides. A home absence enters with a minus sign, an away absence with a plus sign. Each player's coefficient is what his team loses per game without him, in points.
3. The regression has a prior that pulls every coefficient toward zero (ridge regression). Its strength is the ratio of the residual variance to the prior variance of a worth. The prior spread is estimated from the data. It is the spread of the raw with/without differences of the players with at least five absences and five games played. The noise those differences carry is taken out, and the result is bounded between 1 and 6 points. With fewer than ten such players the default of 3 points stands. The page says which.
4. Each worth has a standard error from the same regression. The page shows the worth in Elo (the points times 28), the error, and the raw means with and without the player.

The regression values a player only through variation. Two players out for the same games cannot be told apart: the ridge splits the gap between them. A player who never misses a game has no estimate. A handful of absences stay near zero with a wide error, because one game's residual has a wide spread. The typical NBA figure is about 12 points. The reading is therefore strongest for a rotation player who missed a block of games in a season the team otherwise played whole.

# Availability

The latest injury report stored gives a status for each listed player, and sometimes a return date. The forecast prices it as follows:

| Status | Share of the worth taken off | Span without a return date |
|--------|------------------------------|----------------------------|
| Out | all | 14 days |
| Out for season | all | the rest of the season |
| Doubtful | three quarters | 3 days |
| Questionable, day-to-day, game-time decision | half | 2 or 3 days |
| Probable | a quarter | 1 day |
| Available, active, healthy | none | - |
| Any other wording | none | - |

A status that starts with "out" counts as out. So does one that mentions a suspension, a player not with the team, or "not available". A status that mentions the season counts as out for the season.

With a return date the span ends the day before it. The worth is the player's estimate with the smallest standard error among the forecast's season and the two seasons before it. Ties go to the latest season. One absence this season therefore does not displace a full season before it. A player with no estimate takes nothing off. A report dated more than 30 days before the season's first game, or after its last, describes another season. It is not priced, and the response says so. A worth at or below zero gives no penalty: the model does not reward an absence. A penalty is capped at 400 Elo.

The season engine subtracts each team's penalties from its rating for the simulated regular-season games in the span, for the result only. The rating update of a simulated game works from the ratings themselves, as the real update does. The Season Forecast page's **Use the injury report** box fills the forecast's `absences` from the report at submission. The stored parameters therefore say what was priced in. The team table shows each team's mean penalty over its remaining games. The Players page lists the report with each line's worth and penalty. The season model version is `elo-season-1.3` from this change.

# Limits

* The estimate is noisy by nature and honest about it: read the error beside the worth. A worth without an error is not shown.
* The ratings adapt during an absence, because the K-factor moves them game by game. This understates the worth of a player out for a long stretch.
* Injuries cluster, and a team that lost one player often lost others. The regression shares the gap, but it cannot see a player who never sits. The [box-score worth](box-score-worth.md) is the second estimate for him, from his production.
* The report is a snapshot. A return date is the source's expectation. A player listed as day-to-day who then misses a month is priced for three days.
* Only the simulated regular-season games take the penalty. The play-in and the playoffs run at the full ratings. A player out for the season still counts for his team in April.
* Nothing here changes a rating. A later change may use the worth in the ratings themselves.
* On 2026-10-26 the method was tested on synthetic seasons only, because the sandbox holds no box scores. The first reading on real seasons is the check of the prior spread and of the status weights.
