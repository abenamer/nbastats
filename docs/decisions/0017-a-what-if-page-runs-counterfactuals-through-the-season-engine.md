---
type: Architecture Decision
title: A What if page runs counterfactuals through the season engine
description: Why a real season can be run again with a team's rating set by hand, a team barred from the postseason, another era's playoff format, players out or a player's worth changed, as a second run of the season engine beside the plain one, and what the engine takes as parameters for it.
tags: [decision, season, counterfactual, what-if]
generated: { by: "human:abenamer", at: "2026-11-05T14:00:00Z" }
---

# ADR 0017 — A What if page runs counterfactuals through the season engine

## Context

The season forecast answers "what happens from here". The maintainer also wanted "what would have happened if": a team taken out of a season or replaced by an expansion team, a team rated as it was in another year, the top-eight rule of the 2010s in a play-in season, a best-of-five first round, a star out for the year, a player worth more than the model says. The engine already took absences, rating shifts and worth what-ifs ([ADR 0011](0011-a-worth-what-if-moves-the-team-by-the-difference.md)); the format, the qualification rule and the team list were fixed by the season.

## Decision

* The season parameters gain four counterfactual settings, each leaving the season as it was when empty: `rating_overrides` (a team's rating at the cutoff set to a value from 800 to 2,500 Elo, at most 64 teams, applied after the shifts, so it replaces the result of the injury report and the what-ifs), `excluded_teams` (teams barred from the postseason, at most ten), `qualification` (the top eight or the play-in in place of the season's rule) and `first_round_games` (best of five or seven). They travel in the stored parameters like the others, and the season model version does not change, since a run without them is the same forecast.
* A barred team plays its regular-season games as scheduled and keeps its record; it is ordered last in its conference, in its place among the barred, so the next team takes its seed, and the kept teams alone are seeded (a barred division winner takes no protected seed in the seasons through 2015). Its rows carry `excluded`. A team id that is not in the season is refused, not ignored. A conference must keep eight teams for the top-eight rule and ten for the play-in, or the run is refused. "Remove a team" on the page is a rating of 1,300 Elo, the rating of an expansion team, and the bar together: the team's games are still played, by a weak stand-in, so the other teams' schedules keep their length. Dropping the team's games instead was rejected: the standings would count unequal games, and the engine orders them by wins.
* A new page under the Season menu, **What if** (`/season/whatif`), runs the plain season and the changed one from the same date with the same seed and iterations, so what moved between them is the change and not the dice, and shows per team the ratings, the expected wins, the playoff, Finals and title chances of both runs with the move in points, and the changed run's projected bracket. The players-out and worth what-if controls are the Season Forecast page's, shared.

## Consequences

* A counterfactual on a past season uses the ratings and the schedule of that season at the cutoff; the games played before the cutoff keep their results, so a what-if from the preseason re-runs the whole season and one from a date mid-season keeps the standings to that date.
* Two runs cost two jobs; the page runs them one after the other, 5,000 simulated seasons each by default, and the job runner serves them in turn with the others.
* The rating set replaces the team's own rating in the first game to come; with a regular-season K the rating then moves with the simulated results as any other does.
* The format of another era is applied to this season's schedule: a top-eight rule in a season with 30 teams, or a best-of-five first round with today's home patterns, is a counterfactual and not a reconstruction of that era.

## Alternatives

* Replacing a team by another franchise's team of another season (its rating and roster) was rejected for now: a team is its games, and the schedule has none for a team that was not there; a rating set by hand covers the strength, which is what the engine prices.
* Changing the schedule (a shorter season, a different number of games) was rejected: the engine simulates the stored games, and a schedule is not a parameter.
* A separate counterfactual engine was rejected: the point is to run the same engine with the same rules, so the what-if is comparable to the forecast.
