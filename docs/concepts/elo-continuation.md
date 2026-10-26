---
type: Model
title: Elo continuation
description: How the application computes every Elo rating itself from the game results, chains the seasons, validates against the archive, and projects a season that has not started.
tags: [elo, data, projection, continuation]
generated: { by: "human:abenamer", at: "2026-10-21T20:00:00Z" }
sources:
  - id: fte-method
    resource: https://fivethirtyeight.com/features/how-we-calculate-nba-elo-ratings/
    title: How FiveThirtyEight calculates NBA Elo ratings
  - id: code
    resource: /src/domain/continuation.rs
    title: Continuation module
---

# Compute first

The application computes every rating itself, season after season from 1946-47, from the game results with the formulas of the source.[^fte-method] The archive (see [Historical data](historical-data.md)) supplies results and schedules through the 2024-25 regular season; its own ratings are stored beside the application's as reference values and are used for validation only. Every page and engine reads the application's ratings. Later results come from a results source or an upload, and chain from the same computation.

The rule was decided in [ADR 0004](../decisions/0004-compute-first.md): one computation owns every number, and an archive correction can never leave two rating chains in one table.

# Formulas

The application verified these formulas against rows of the archive. They reproduce the archive to four decimal places.

| Step | Rule |
|------|------|
| Season start | `E_start = 0.75 × E_end_previous + 0.25 × 1505`, where the previous end rating is the franchise's (a relocated or renamed team continues its lineage). A franchise without a previous season starts at 1300; the eleven clubs that joined from the NBL and ABL in 1948 to 1950 start at the archive's rating for them, because the archive seeded them from their previous league. |
| Home-court bonus | 100 points for the home team. Neutral games get no bonus. |
| Game probability | `P(home) = 1 / (1 + 10^((E_away − E_home − 100) / 400))` |
| Margin multiplier | `((margin + 3)^0.8) / (7.5 + 0.006 × winner_advantage)`, where `winner_advantage` is the rating difference, with the bonus, from the winner's point of view. |
| Update | `E_home' = E_home + 20 × multiplier × (result − P(home))`. The away team moves by the same amount in the other direction. |

A game without a result does not change the ratings. The application records the ratings before the game, so the forecast engines can use the schedule.

# Validation against the archive

The archive carries FiveThirtyEight's rating before and after every game; the application keeps them in the reference columns of each archive game. On 2026-10-23 the full computation over the whole archive (80 seasons, 75,913 archive games, the ABA included) was compared with those reference values on the maintainer's database:

* Mean difference 0.007 points. 632 games differ by more than 0.1 points and 121 by more than one point, all in six seasons.
* The differences trace to the archive's own corrections, where its rating before a game is not its rating after the team's previous game: 2007-08 (the Atlanta and Miami game of 2007-12-19 whose last 52 seconds were replayed on 2008-03-08; the archive moves both teams by about 10 points and back), 2020-21 (Denver and Detroit, 2.49 points) and 2021-22 (Cleveland and Sacramento, 2.70 points), plus one game, the 2015 Finals game of 2015-06-14, where the archive's update is 1.28 points smaller than the formula gives. The application carries every game forward consistently, and the small season-start differences of 2015-16, 2021-22 and 2022-23 follow from those corrections through the reversion.
* On neutral-site games the one-step difference is at most 0.08 points.

The integration test `elo_replay_reproduces_the_archive_ratings` repeats the one-step and the chain check on the 2015-16 fixture on every build and fails above 0.1 points.

# Three ways to add a season

| Method | Input | Result |
|--------|-------|--------|
| Results sync | The BALLDONTLIE API (an API key in `BALLDONTLIE_API_KEY`). The first sync of a season fetches every game, schedule included. Later syncs fetch only the days from the last played game to today and keep the stored games outside those days (see [Import and refresh data](../runbooks/import-data.md)). | The ratings follow the results. The games record `results` as their result source. The import log records the source URL. |
| Results import | A CSV file in the results layout with the played games, and optionally the games that are not played yet. | The ratings follow the results. The games record `results` as their result source. |
| Preseason projection | None. | The previous season's regular-season matchups, one year later, with no scores. All ratings are the reverted ratings of the previous season. |

A projection is a placeholder. Its schedule is not the real schedule. Use it to run the [Season forecast engine](season-engine.md) before the real schedule is available, then replace it with a results import.

# Rules

* A results import, a sync or a projection stores the season's games again: a game the incoming data holds is updated in place, a non-archive game it no longer holds is deleted, so a game keeps its identifier and its box-score lines. Then every rating of the season is computed again from its start ratings, archive games included. The archive games keep their rows and receive the new ratings; the replacement and the ratings are written in one transaction, so a failure leaves the season as it was.
* Archive games always stand. An incoming game that matches an archive game by its date and its two teams, in either order (sources disagree on the home team at a neutral site, such as the NBA Cup final), is dropped. A source or file that adds no game beyond the archive stores nothing. A projection or a removal of a season with archive games is refused.
* After you complete a season, sync the next season again: its start ratings come from the final ratings of the completed season.
* An archive import stores the results and the reference ratings, then computes every season's ratings again from the first season, because an archive correction can move every later chain. The ratings of every season are written in one transaction: when the computation fails, no rating changes, the import is marked failed, and the games stay stored for a later `recompute`. The identical file imported again changes nothing.
* The application needs the previous season in the store, or archive games of the season itself to seed from. The start rating of each franchise comes from its last game of the previous season.
* Team codes must exist in the reference table. Unknown codes stop the import with a message.
* The BALLDONTLIE adapter maps three abbreviations to the dataset's codes: BKN to BRK, CHA to CHO, PHX to PHO. A game counts as played only when its status is `Final`. A game whose Cup stage is the final (given by the source from 2025-26) is stored as a Cup final, rated like any game and kept out of the standings. A Cup final keeps its type through the next sync and the derived rebuild; play-in games are classified again from the game counts. The adapter paces its requests to the plan's quota and waits when the API answers with status 429.
* A results sync updates the season's non-archive games in place and deletes only the games the source no longer lists. The box-score lines therefore stay attached. A completed daily results sync is followed by the player sync when `PLAYER_SYNC_ENABLED` is true: the season's box scores, today's injury report and the rosters; see [Player data](/concepts/player-data.md). The ratings do not use them yet.

# Limits

* The ratings are the application's own output. They reproduce FiveThirtyEight's ratings to the differences listed under Validation, but they are not FiveThirtyEight's numbers; those stay in the reference columns.
* The margin-of-victory formula uses the regular-season K-factor of 20 for playoff games too. The archive applies the same K in the playoffs.
* Derived champions and seeds follow the same rules for every season. A season with no playoff results has no derived champion.
