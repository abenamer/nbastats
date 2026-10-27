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

1. Every played game has a residual: the actual margin minus the margin the pre-game ratings expected. The expectation uses 28 Elo points per point of margin and the ratings' own home bonus of 100. The 28 is FiveThirtyEight's rule for the ratings the application replays. A rating gap of 28 is a one-point spread, so the home bonus of 100 is about 3.5 points. It is one constant for every era, although a point of margin meant more in the low-scoring seasons. The residual holds everything the ratings did not know, the line-ups included.
2. The residuals of the games with an absence are regressed on the absences of both sides. A home absence enters with a minus sign, an away absence with a plus sign. Each player's coefficient is what his team loses per game without him, in points.
3. The regression has a prior that pulls every coefficient toward zero (ridge regression). Its strength is the ratio of the residual variance to the prior variance of a worth. The prior spread is estimated from the data. It is the spread of the raw with/without differences of the players with at least five absences and five games played. The noise those differences carry is taken out, and the result is bounded between 1 and 6 points. With fewer than ten such players the default of 3 points stands. The page says which.
4. Each worth has a standard error from the same regression. The page shows the worth in Elo (the points times 28), the error, and the raw means with and without the player.

The regression values a player only through variation. Two players out for the same games cannot be told apart: the ridge splits the gap between them. A player who never misses a game has no estimate. A handful of absences stay near zero with a wide error, because one game's residual has a wide spread. The typical NBA figure is about 12 points. The reading is therefore strongest for a rotation player who missed a block of games in a season the team otherwise played whole.

# Availability

The latest injury report stored gives a status for each listed player, and sometimes a return date. The forecast prices it as follows:

| Status | Default share of the worth taken off | Default span |
|--------|--------------------------------------|--------------|
| Out | all | 14 days |
| Out for season | all | the rest of the season |
| Absent (not on the report, see below) | all | 14 days |
| Doubtful | three quarters | 3 days |
| Questionable, day-to-day, game-time decision | half | 2 or 3 days |
| Probable | a quarter | 1 day |
| Available, active, healthy | none | - |
| Any other wording | none | - |

A status that starts with "out" counts as out. So does one that mentions a suspension, a player not with the team, or "not available". A status that mentions the season counts as out for the season, and so does a description that says the player is done for the season ("out for the season", "done for the season", "miss the rest of the season", "season-ending") under a status that does not; a note about the season opener, debut or finale, or an injury avoided, does not count.

The shares and spans in the table are the defaults. When the stored reports show what a status has meant, the measured share and span replace them; see [What a status has meant](#what-a-status-has-meant).

## How long an absence lasts

The feed's return date is most often the date of the team's next game, whatever the injury: a player who will miss two months reads as out until tomorrow, every day, until a writer puts a date on it. The forecast therefore does not take the date at its word. The rule, in `span_for`:

1. Out for the season ends with the season.
2. A return date beyond the team's next regular-season game is the source's estimate. The span ends the day before it, at the status's default weight: the measured weight is read over every line of a status, dated or not, and does not apply to a line the source has dated.
3. A return date that is the next game or earlier is a placeholder, and so is no date. A player listed out, or absent, or in a run of three or more missed games whatever his status, is then priced for the expected remaining games of a run as long as his, laid over his team's remaining regular-season games. The expectation comes from the survival curve of the runs of missed games: every run of a regular player in the five seasons before, with the runs cut off by the end of a season counted for the games they did last (Kaplan-Meier, read as ending at the longest run seen, which keeps the expectation for a very long run on the low side). A run that has lasted ten games has more to go than a new one; the curve says how much. The rule needs at least 50 runs and the games missed so far; with fewer runs, or for a team without box scores, the status's default span stands.
4. Any other line whose status has a curve (see below) is priced for the games a line of its status, in the player's state, has gone on to miss, laid over the schedule the same way.
5. Without a curve to lean on, a return date of today prices nothing for a line about tonight (questionable, doubtful, day-to-day, probable); those lines keep their default spans. With a curve, or for a player in a spell, it is the placeholder of rules 3 and 4.

The games a player has missed so far are read from the box scores: his run at his team's latest game with lines, plus the games the team played since without lines, which nobody has a line for. The Players page shows them beside each line, and what set each span and each weight.

## Rookies: the draft prior

A player with no stored regular-season line has no worth from either estimate, and a first pick is not nothing. The application measures what a draft slot has been worth, in `domain::rookie` (model `draft-prior-1.0`):

1. Over the six seasons before the one priced, every rookie (drafted the year before his first season) gives one sample: his box worth in that season, weighted over his stints by minutes.
2. The rookies are pooled by draft bracket: the first pick, picks 2 to 5, picks 6 to 14, the rest of the first round, the second round and later, undrafted. A bracket with at least 10 rookies gets a prior: the mean of their first-season box worth, the error of that mean (the spread over the square root of the count) and the spread itself. The first pick gives one rookie a season at most, so picks 2 to 5 stand in for it when it has too few, picks 6 to 14 for those, and the rest of the first round for those; the second round and the undrafted have no stand-in. Nothing is fitted per player.
3. A rookie on the report (drafted the year before and not seen in a stored season before this one, or undrafted and not seen before) is priced by the prior when he has no estimate: the mean, with the error of the mean as his error. Once he has a box worth this season, it is pulled toward him by his minutes over his stints: at 500 minutes the prior and his own worth weigh the same, and the error shrinks in step. A rookie with a worth from the missed games keeps it. A prior within one error of zero prices nothing, as a worth from the missed games does: the box worth stands when he has one. A rookie whose bracket has no prior and no stand-in keeps his box worth or stays without an estimate.

The Players page lists the priors in the panel "What a draft slot has been worth as a rookie", with the spread beside the error, and a rookie's line names the bracket that priced him. The measured figures are the means, their errors and the spreads. The choices are the bracket edges, the minimum of 10, the six-season window, the minutes weighting over stints, the stand-in order and the 500 minutes at which the prior and the player weigh the same.

## What a status has meant

A status is a word from the feed. "Day-to-day" on the first morning of a sprain and "day-to-day" ten games into a fracture are the same word and different facts, and the default weights priced both at half the worth for three days. The application therefore measures the statuses on its own data, in `domain::status` (model `status-curve-1.0`):

1. Every line of every stored injury report whose status takes something off, out for the season apart (that one is priced by definition), is followed through the box scores of the player's team: did he miss the team's next game, and which of the next thirty did he miss. A line whose team has no later game with box scores yet gives nothing; a line is cut off where the box scores end. A player who has not played for the team yet counts, and reads as not yet out until he has a played game with the team, as the pricing reads him.
2. Each line is read in the player's state on the report day: not yet out (he played the team's last game before the report) or already in a run (he had missed it). The two states are kept apart.
3. For each status and state with at least 50 lines, the curve is the share of the lines whose player missed the next game, the one after, and so on. The weight is the first share: the measured chance he misses tonight. The expected games missed is the sum over the thirty. The span, in games, is the expected games missed divided by the weight, so that weight times span is the games missed; it never runs past the thirty games followed. Below 50 lines the default weight and span of the table above stand. A status measured as never missing the next game prices nothing.
4. The lines come from the season being priced and the five seasons before it, pooled. The curves are kept per status and state, not per player, so no player is fitted on his own history.

Which weight and which span priced each line is shown beside it on the Players page (`measured` or the default), and the panel "What a status has meant" shows every curve: the lines behind it, the share missing tonight against the default, the games missed of the next thirty, the span, and the share still out after one, five and ten more games. The question box's `team_availability` tool carries the same figures.

**Absent without a line.** A regular player (fifteen of the last twenty team games, at fifteen minutes a game, so the rule starts after a team's twentieth game with lines) who has not played his team's last three games or more and is not on the report is priced as out, under the status "absent", with the same rule. The report misses some players; the box scores miss none who dressed. The reason does not matter to the forecast: the team plays without him either way. A player who has left the team is no absence: one the last roster refresh did not list, or whose current team is another, is dropped. A season whose lines carry no minutes gives no runs and no absent players.

The worth is the player's estimate with the smallest standard error among the forecast's season and the two seasons before it. Ties go to the latest season. One absence this season therefore does not displace a full season before it. When that estimate stands within one standard error of zero, or when there is none, the [box-score worth](box-score-worth.md) prices the line instead. It is taken from the latest of those seasons, and it carries no error. The Players page says which estimate each line used, with its error. The penalty it shows is the fixed one; a forecast that draws the worth applies the mean of the held draw, slightly higher for a noisy estimate. A player with no estimate of either kind takes nothing off. A report dated more than 30 days before the season's first game, or after its last, describes another season. It is not priced, and the response says so. A worth at or below zero gives no penalty: the model does not reward an absence. A penalty is capped at 400 Elo.

The season engine subtracts each team's penalties from its rating for the simulated regular-season games in the span. The subtraction applies to the result and to the expectation of the rating update. The simulated rating therefore does not slide during a known absence. The real rating does, because the real update knows nothing of injuries. A later change may add that drift back when a player returns. The Season Forecast page's **Use the injury report** box fills the forecast's `absences` from the report at submission. The stored parameters therefore say what was priced in. The team table shows each team's mean penalty over its remaining games. The Players page lists the report with each line's worth and penalty. The season model version is `elo-season-1.3` from the absences and `elo-season-1.5` from the update's expectation.

# Players marked out

The Season Forecast page also takes absences by hand: a player, a span of dates and an estimate. **Auto** takes the player worth of the season when he has one, else his [box-score worth](box-score-worth.md). The other choices force one or the other. The player worth comes with its standard error; the box worth has none. An estimate at or below zero adds nothing. One span per player: a second entry for the same player replaces the first. A player marked out replaces his line in the injury report when the report is used too, and the rest of the report is added.

With **Draw each worth from its uncertainty** ticked, every simulated season draws each absence's penalty from its standard error. A player with few absences then widens the forecast rather than moving it with false precision. Without it, the penalties are fixed at their estimates. The result says how many absences were priced and how many were drawn. Nothing here is stored outside the job's parameters.

# Limits

* The estimate is noisy by nature and honest about it: read the error beside the worth. A worth without an error is not shown.
* The ratings adapt during an absence, because the K-factor moves them game by game. This understates the worth of a player out for a long stretch.
* Injuries cluster, and a team that lost one player often lost others. The regression shares the gap, but it cannot see a player who never sits. The [box-score worth](box-score-worth.md) is the second estimate for him, from his production.
* The report is a snapshot, and its return date is a placeholder more often than an estimate. The expected remaining games correct the typical case; a particular player returns earlier or later than the curve says, and the forecast does not yet draw that length from the curve as it draws the worth from its error.
* An absent regular player is priced whatever the reason: a suspension, a rest or a coach's decision look like an injury. The team plays without him either way.
* The curves are read over each line's team, whether or not the player has played for it yet, so a player who joins or leaves the team mid-season is counted for the team the report names. A report fetched after the day's game reads that game as the next one, where the pricing takes the next unplayed game. The in-run curve pools runs of one game and more; a player in a spell of three or more takes the survival curve's span and the in-run curve's weight.
* The draft prior is what rookies of a slot have been worth on average; it says nothing about the player himself until his minutes do, and its error is the error of that average, not the range of the rookies, which runs far wider. The first pick's own sample is almost always too small, so a first pick is priced as a top-five pick.
* The status curves rest on the reports stored since the feed was connected, so they fill in over time and the first seasons lean on few lines. A player who leaves his team (traded, waived) reads as missing every game that follows, which runs the long end of a curve high; the thirty-game horizon bounds it. A status whose meaning drifts from season to season is averaged over the seasons pooled.
* Only the simulated regular-season games take the penalty. The play-in and the playoffs run at the full ratings. A player out for the season still counts for his team in April.
* Nothing here changes a rating. A later change may use the worth in the ratings themselves.
* On 2026-10-26 the method was tested on synthetic seasons only, because the sandbox holds no box scores. The first reading on real seasons is the check of the prior spread and of the status weights.
