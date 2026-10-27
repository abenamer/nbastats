---
type: Model
title: Season forecast engine
description: Engine C simulates the rest of a season from a cutoff date, gives every team's probability for each stage and position, and draws the most likely bracket with its own small probability.
tags: [engine-c, season, play-in, forecast]
generated: { by: "human:abenamer", at: "2026-10-21T17:30:00Z" }
sources:
  - id: code
    resource: /src/playoffs/season.rs
    title: Season engine source
---

# Purpose

The Season Forecast page shows, for every team, the probability to:

* finish in the top six of its conference,
* play in the play-in tournament,
* reach the playoffs (first round),
* reach the second round,
* reach the conference finals,
* reach the NBA Finals,
* win the championship.

# Procedure for one simulated season

1. Take each team's Elo after its last game on or before the cutoff date. Before the first game, take the preseason rating from the data.
2. Count the standings from the games played on or before the cutoff.
3. Simulate every later regular-season game from Elo, with the season's regular-season home-court bonus (fitted on the previous five seasons unless you set one; see the [Elo rating model](/concepts/elo-model.md)). Update ratings with the regular-season K-factor (default 20).
4. Order each conference with the season's tiebreak rules. Ties use the group head-to-head record, then point differential, then a seeded random key.
5. Resolve the play-in with a state machine:
   * 2020-21 and later: 7 hosts 8; 9 hosts 10; the loser of the first game hosts the winner of the second game.
   * 2019-20: the ninth team plays the eighth team when it is within four games. The ninth team must win twice.
   * Before 2019-20: the top eight qualify.
   A play-in game that was played on or before the cutoff keeps its real result.
6. Build the bracket and play it with the playoff model and the playoff home-court bonus (default 100). Playoff ratings are fixed unless you set a playoff K-factor.
7. For a season with an NBA Cup, build the group tables from the group games, seed the knockout and play it; see the [NBA Cup forecast](nba-cup.md).

# Projected bracket

The forecast is a distribution over thousands of brackets, not one bracket. The page still draws one, the projected bracket, and says how unlikely it is:

1. For each conference and each seed in turn, from 1 to 8, the slot goes to the team that held that seed most often in the simulated seasons among the teams not yet placed. When every team that held a seed is already placed, the seed goes to the unplaced team that made the playoffs most often. The seeds are the seeds after the play-in, when the season has one.
2. Each team shows its projected final record (its expected wins, rounded). A higher seed can therefore show a worse record than a lower one.
3. The probabilities through the bracket are exact: each team's chance of winning each round and each first-round series, from the playoff model with its home-court bonus and scale, at the ratings of the cutoff date, with fixed ratings (no playoff K-factor). The bracket is drawn through every round, each conference's three rounds converging on the Finals: a later-round slot shows the team more likely to have won the round before, marked projected, with its chance of winning the round from the start, and the champion box shows the likelier finalist with its title chance. The Cup's projected knockout is drawn the same way, through its quarterfinals, semifinals and final.
4. The line under the bracket says how often this exact seeding occurred, how often these sixteen teams made the playoffs in any seeding, and how often the single most frequent seeding occurred. The counts cover the first 200,000 simulated seasons, which bounds the memory of the counting. These shares are small. The bracket is one draw from the distribution, chosen for being the most plausible slot by slot. The stage probabilities in the table above it remain the forecast.

Two heat maps show the distribution the bracket is drawn from: each team's chance of each regular-season position in its conference, and of each playoff seed after the play-in when the season has one. The season model version is `elo-season-1.1` from this change; a result stored under 1.0 has no projected bracket.

# Availability

A forecast may carry absences: a rating penalty per team over a span of dates. They come from the latest injury report priced by each player's worth, or from players the user marks out; see [Player worth and availability](player-worth.md). The engine subtracts a team's penalties from its rating for the simulated regular-season games the span covers. The subtraction applies to the result and to the expectation the rating update works from. A known absence is a temporary condition, not a change in the team's quality. It moves the standings and not the rating, and the team plays at its full rating from the day the absence ends. Through version 1.4 the update expected the full rating; version 1.5 expects the penalized sides. Before, a team under-performed its rating during an absence, its simulated rating slid, and it never recovered. In one forecast a two-month absence cost about twice the wins the arithmetic supports; the engine's own test shows about a third more. The play-in and the playoffs run at the full ratings. When absences were applied, the team table shows the rating each team plays at on the cutoff date and its mean penalty over its remaining games. The result counts the absences applied. Two absences of one team add up; two absences of one player with overlapping spans are refused.

**The held run.** The dated forecast depends on when each player returns, and the return dates are the weakest input: the feed's placeholder, a writer's guess or a curve. A forecast with `hold_absences` set runs the season a second time with every absence that covers the cutoff date and ends before the season's last regular-season game held to that game: the forecast if the roster of the cutoff date were the roster all season. The result carries it as `held`, per team: the mean penalty, expected wins and the chances of the top six, the play-in, the playoffs, the conference finals, the Finals and the title, in the held run's order (join on the team identifier). With the forecast with nobody out, which the question box runs and the page does not, it brackets the dated forecast: the truth lies between the two bounds, up to about a point of simulation noise, and the dated forecast says where on the current estimate of the return dates. Absences that start after the cutoff date keep their spans. With no absence to hold there is no held run and the forecast is the single run. The hold is decided before the first run, so a cancelled job stops after it. The second run takes as long as the first, and the job's work estimate doubles with the flag whether or not a held run follows; the stored parameters are the dated ones with the flag, and `held_to` records the hold. The Season Forecast page's **Hold the cutoff date's absences all season** box asks for it and shows the held columns beside the dated ones.

**The injury card.** The mean penalty in the team table is an average over the remaining games, so a star out for a week moves it by a few Elo while the team plays tonight at a hundred below its rating. The card lays the arithmetic out for one team, in `application::worth::injury_card`: every line of the latest report priced for the team, with the worth and its source, the weight and whether it is measured, the penalty per game (worth times weight; with an error on the worth, the games are priced at the mean of the draw held between zero and the cap, as the forecast prices them, and the card shows both when they differ), the span's end and what set it, the remaining regular-season games the span covers, the penalty summed over them (Elo times games), that sum divided by the remaining games (the line's part of the mean) and the expected wins lost to the line alone. Wins lost is the win probability at the team's current rating less the probability at the penalized one, game by game over the covered games, at today's ratings with the opponents at full strength and the season's regular-season home bonus (`domain::worth::absence_effect`): a first-order figure the forecast does not use, since it simulates the season instead. The totals come two ways: as listed, with each absence ending when the report, the curve or the status default says; and held, with every absence that covers today kept to the last regular-season game (`domain::worth::held`), which is what the held run simulates. As listed is the dated estimate; held is the upper bound, with the forecast at full strength as the lower. The card is read as of today, or as of the report date for a season already over. The page shows the card for the team picked, and a team's code in the forecast table opens it. `GET /api/seasons/{season}/teams/{code}/injury-card` returns it.

**Tonight.** The question "what does the team play at tonight" has one answer and the season average is not it. The Tonight panel at the top of the page (`application::worth::tonight`, `GET /api/seasons/{season}/tonight`) takes the next night with an unplayed regular-season game on or after the date it is read as of (today, or the report date for a season already over) and prices each game at the ratings the teams play at that night: the rating as of the date less the penalties of the absences that cover the night, with the home win probability at full strength and at those ratings (`domain::worth::tonight`). The team table lists every team of the season by its rating tonight, with the penalty (the sum over the absences covering the night of the penalty each carries, the mean of its draw when the worth has an error), whom it plays and who is out with that penalty and the last date it applies. The night is today's when a game of today is still to play, else the next date with one, and the panel says which and how many of the night's games are already played; a team whose game is played shows no opponent. When no regular-season game remains the panel prices nothing: the playoffs run at the full ratings. A team's code opens its injury card.

An absence may carry the standard error of its penalty. When it does, every simulated season draws the penalty from a normal distribution around the estimate, held between zero and the cap. The forecast's spread then reflects how little is known about the worth. The reported penalty is the mean of that held draw. For an estimate well inside the range it equals the estimate. An estimate within about two errors of zero is pulled upward, because the draw stops at zero. An estimate of 0 with an error of 60 applies about 24 Elo on average. An error of zero gives the fixed penalty, and the same seed then gives the same forecast as before. The result counts the absences sampled. The season model version is `elo-season-1.5` from this change (1.3 introduced the absences, 1.4 the drawn penalties).

# Information cutoff

Only games on or before the cutoff date inform the forecast. A cutoff inside the playoffs is moved back to the day before the first playoff game. The Season Forecast page defaults to today's date for the season in progress, and to the preseason for a past season. The integration test `forecasts_never_use_future_information` checks that a forecast does not change when later games are removed from the input.
