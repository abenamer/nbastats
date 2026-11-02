---
type: Architecture Decision
title: A worth what-if moves the team by the difference
description: Why a reader's what-if on a player's worth, typed or read from a window of his recent games, is applied as a rating shift of the difference from the model's estimate on his team, plus his absences priced at the what-if, in the forecast, the Tonight panel and the injury card, without changing the model.
tags: [decision, what-if, worth, player]
generated: { by: "human:abenamer", at: "2026-11-02T16:00:00Z" }
---

# ADR 0011 — A worth what-if moves the team by the difference

## Context

The maintainer watched a rookie play like a star for six games while the model, which reads results at K = 20 (a half-life of about 25 games for a step change in a team's strength) and prices a rookie's worth from a draft prior pulled toward his box score by his minutes, still carried last season's view of his team. The market had moved. The maintainer did not want the model changed, but wanted to ask it: what if this player keeps playing as he has in his last few games? He does not know how a stat line translates to Elo, so the what-if has to read it for him.

A player's worth, in this model, is what his team loses when he sits ([Player worth](../concepts/player-worth.md)). It is not added to the team when he plays: the team's rating already holds his play, as far as the results have shown it.

## Decision

* A what-if is a `WorthOverride`: a player and either a worth in Elo, from −400 to 400 (a player may be taken to hurt his team), or a window of his last 1 to 82 played regular-season games of the season; at most sixteen per request. A window is read through the model's own conversion of a stat line to Elo: the box-score worth table (`domain::bpm`) built from those games alone, all their lines included, so the team adjustment and the league figures are those games', giving BPM over replacement per game on the box scale, then scaled by the season's box calibration to the worth scale, as the injury report's box worths are. One game is noise and the page says so; the resolved what-if names the window, the BPM, the minutes and both figures. `application::worth::what_ifs` resolves each against the season: the model's own estimate of the player (the same pricing the injury report uses), his current team from the last roster refresh, and the shift the team's rating takes: the what-if less the estimate, or the whole what-if for a player the model cannot price. A player the store does not know, or one without a current team, answers 400.
* The forecast takes `worth_overrides` in its parameters. The application adds each what-if's shift to the team's `rating_shifts` (on top of the full-strength shift, the sum held within 400 Elo either way) and prices the player's absences at the what-if, before the run. The engine reads nothing from the what-ifs; they stay in the stored parameters so the forecast says what it priced.
* The Tonight panel, its market and the injury card take the same what-ifs as `worth=id:elo,...` or `worth=id:lastN,...` on the request and apply them the same way: the team's full-strength rating moves by the shift, the player's absences cost the what-if. Each answer carries the resolved what-ifs, so the page can say "at 60 Elo, the model's estimate 7, so the team +53".
* The tracking captures without what-ifs. The record judges the model, not the reader.

## Consequences

* A reader can test a belief about a player in every place the model prices a team, and read the model's own estimate beside it.
* The model does not change: its ratings, worths and the record stay what they are. A what-if is a reader's input, named as such on the page and in the stored parameters.
* The shift is a crude thing by design: a constant added to a team's rating for the forecast's remaining games, which is what "the team is better than its results show" means in an Elo model. It does not make the rating learn faster.

## Alternatives

* Raising K early in the season, or reading the box scores into the team rating, was rejected: the maintainer asked that the model stay as it is, and the record has not yet said whether the market or the model reads these cases better.
* A what-if from a window of games only, with no typed worth, was considered and both are kept: the window answers the question the maintainer asked, and a typed worth lets a reader test a belief the box score has not shown yet.
* A separate what-if page was deferred: the page the maintainer reads already holds the player search and the forecast, so the what-if joins it.
