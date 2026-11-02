---
type: Architecture Decision
title: Cup games from the source's stage labels
description: Why the season engine tells the NBA Cup games of a season by the stage labels the results source puts on each game, with the curated dates kept for the seasons before the labels and as the fallback, and why a stored Cup season without labels is fetched whole once.
tags: [decision, cup, results, sync]
generated: { by: "human:abenamer", at: "2026-11-02T18:00:00Z" }
---

# ADR 0012 — Cup games from the source's stage labels

## Context

The Cup forecast ([NBA Cup forecast](../concepts/nba-cup.md)) told the Cup games of a season by their dates: a regular-season game on a group night was a group game, a game on a knockout date a knockout game, and the dates were a curated record per season in `domain::cup`. Each new season needed its dates typed in from the league's announcements and checked against the calendar, and a season without them had no Cup forecast. With the first Cup night of 2026-27 played and no dates recorded for it, the maintainer asked whether the feed carries the Cup games and whether the game already played would be caught.

The results source labels each Cup game with its stage (`ist_stage`: `East Group A` to `West Group C`, `East Quarterfinal`, `West Quarterfinal`, `East Semifinal`, `West Semifinal`, `Championship`) from the 2025-26 season on. The application already read the label to tell the final from a regular game, and dropped the rest.

## Decision

* The label is stored. `Game::cup_stage` is a `CupStage`, the source's eleven stages as an enum with the round, the conference and the league's group letter of each; a label the application does not know is stored as none. The `games` table gains `cup_stage` (migration `0008`); the upsert keeps a stored stage when a later fetch brings none for the game, so only removing the season clears a label the source took away.
* The season engine tells the Cup games by their labels when any game of the season carries one: a game labelled with a group stage is a group game, one labelled with a conference quarterfinal or semifinal a knockout game, the `Championship` the final. The group derivation, the tiebreaks and the knockout are unchanged: the labelled group games still have to form six groups of five, three per conference. For a season with no labelled game the curated dates tell the games as before. When a season has both and the labelled group games do not form the groups (a label missing on one night's games) while the dated ones do, the dates stand in, so the one season with both, 2025-26, cannot lose its forecast to an incomplete label set.
* A season stored before the labels were read is refilled once. From the first labelled season (`domain::cup::FIRST_LABELLED_SEASON`, 2025-26), a results sync of a season whose synced games all lack a stage fetches the whole season instead of the days since the last played game. Archive rows never take the source's label and do not count. After that fetch the daily window, the days from the last played game to today, carries each Cup game's label on its day.

## Consequences

* The 2026-27 Cup and the ones after need no curated dates. The curated table keeps 2023-24, 2024-25 and 2025-26, for the seasons before the labels and for a store that has not synced since the labels were read, and is not extended.
* A knockout game the league adds to the schedule in December enters the store on its day, as every scheduled game does under the daily window; until then it is not among the simulated games, as the Cup forecast's limits already say.
* The page's Cup note, for a Cup season without a forecast, says that no Cup game of the season is labelled in the stored schedule and no dates are recorded, and names the two remedies: sync the results, or add the dates.
* The one refill costs a whole-season fetch, about three minutes on the free tier, once per stored Cup season. A season whose schedule the source publishes before it labels the Cup games, or a source that stopped labelling, pays it on every sync of that season until a label arrives; the window rule reads the stored games, not the source, so it cannot tell the cases apart.
* The group letters on the page stay the application's, by the lowest team identifier. The league's letters are stored with each game and can replace them in a later change.

## Alternatives

* Taking the groups from the labels directly, without the pairing derivation, was deferred: the derivation checks that the labelled games form six groups of five, and the labels alone do not. Reading each group's members from the labels and checking them against the derivation is a later change.
* Deriving the dates of each round from the labelled games and keeping the date rule was rejected after review: a game played on a Cup night that is not a Cup game, or a group game moved to another date, would be misread, and the labels make the game-by-game rule cheap.
* Curating the 2026-27 dates by hand was rejected: the source carries them, and typed dates have to be checked against the calendar every season.
* Refilling on every sync, or never, was rejected: once is enough to carry the labels into a store that predates them, and the daily window keeps them current.
