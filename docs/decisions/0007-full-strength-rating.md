---
type: Architecture Decision
title: Full-strength rating from a replayed season
description: Why tonight's absences are priced off a rating whose played games are replayed with the absences each side had, instead of off the stored Elo.
tags: [decision, elo, availability, injuries]
generated: { by: "human:abenamer", at: "2026-11-01T11:00:00Z" }
---

# ADR 0007 — Full-strength rating from a replayed season

## Context

The stored Elo blames the whole team for a game played without its best players. The Tonight panel, the injury card and a forecast with the injury report then took tonight's absences off that rating. An absence that had lasted all season was counted twice: once in the rating, which had already fallen for the games played without the player, and once in tonight's penalty. The maintainer saw it on Golden State's card, where three players out since the season began were charged on top of a rating that had already lost those games.

## Decision

* A pure replay, `domain::full_strength::replay`, goes through the season's played games again from each team's first rating, with the FiveThirtyEight update. Each game's expectation sets each side at its rating less the Elo it was missing that night, and the change goes to the full rating. With no absence it gives the stored ratings back.
* Who a side was missing comes from the stored injury reports, confirmed by the box score (`full_strength::game_absences`). A player the latest stored report within a day of the game listed, who then has no line with minutes, was missing. The report date is chosen across all teams. He counts at the expected penalty of the worth tonight's lines are priced with; a side's total is capped at 400 Elo.
* The application caches the missing Elo per game per season, and clears it with the player caches. The Tonight panel, the injury card and the question box's team tools read the full-strength rating and show the stored Elo beside it.
* A forecast with the injury report stores `rating_shifts`, each team's full-strength rating less its stored one at the cutoff. The engine adds them to the current ratings before simulating. Season model `elo-season-1.6`.
* The stored Elo is unchanged: the game log, the records and the history keep the application's computed ratings ([ADR 0004](0004-compute-first.md)), with FiveThirtyEight's in the reference columns.

## Consequences

* Tonight's absences come off the full team once. A team that has been short-handed all season rates higher than its stored Elo, and its opponents in those games slightly lower.
* The figure depends on the stored reports. A season without them, or games without box scores, keeps the stored ratings. A player missing without a report line is not counted.
* Past games use today's worth estimates, not the estimates of the day.

## Alternatives

* Charging tonight only the players who have not been out all season was rejected: a partial fix that still double-counts a long absence that began mid-season.
* Rewriting the stored Elo was rejected: the stored ratings are the application's own replay of FiveThirtyEight's model ([ADR 0004](0004-compute-first.md)), the records and the history read them, and a rating that depends on stored reports and worth estimates would change them whenever a report or a box score arrives.
* Detecting absences from the box scores alone was rejected for now: the regular-player window needs twenty games, so it misses the first weeks of a season and players who have not played at all.
