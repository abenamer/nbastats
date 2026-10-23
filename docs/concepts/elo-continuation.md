---
type: Model
title: Elo continuation
description: How the application computes Elo ratings for seasons after the end of the imported archive, and how it projects a season that has not started.
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

# Why the application computes ratings

The imported archive ends with the 2024-25 regular season (see [Historical data](historical-data.md)). No public source serves Elo ratings for later seasons. For those seasons the application computes the ratings itself from game results, with the formulas of the source.[^fte-method] The games are marked `computed` so that you can always see which ratings come from the archive and which ratings the application made.

# Formulas

The application verified these formulas against rows of the archive. They reproduce the archive to four decimal places.

| Step | Rule |
|------|------|
| Season start | `E_start = 0.75 × E_end_previous + 0.25 × 1505`. A team without a rating in the previous season stops the import with a message. |
| Home-court bonus | 100 points for the home team. Neutral games get no bonus. |
| Game probability | `P(home) = 1 / (1 + 10^((E_away − E_home − 100) / 400))` |
| Margin multiplier | `((margin + 3)^0.8) / (7.5 + 0.006 × winner_advantage)`, where `winner_advantage` is the rating difference, with the bonus, from the winner's point of view. |
| Update | `E_home' = E_home + 20 × multiplier × (result − P(home))`. The away team moves by the same amount in the other direction. |

A game without a result does not change the ratings. The application records the ratings before the game, so the forecast engines can use the schedule.

# Validation against the archive

The archive carries FiveThirtyEight's rating before and after every game. On 2026-10-23 the application's rating code was replayed once, by hand, over eleven archive seasons from 1947-48 to 2024-25 (the ABA included) and compared with those numbers:

* One step: the application's update applied to a game's imported pre-game ratings reproduces its imported post-game ratings. Mean difference 0.000 points; the largest difference is 0.08 points, on neutral-site games.
* Season start: the reversion rule reproduces every team's imported first rating of the next season exactly.
* Season chain: rating a whole season from its first ratings reproduces the imported season-end ratings. The one exception is 2020-21, where the archive itself moves two teams by 2.49 points between games without a game to explain it (a correction in the source data); the application carries the games forward consistently.

Only the 2015-16 check runs on every build: the integration test `elo_replay_reproduces_the_archive_ratings` repeats the one-step and the chain check on that season and fails above 0.1 points.

# Three ways to add a season

| Method | Input | Result |
|--------|-------|--------|
| Results sync | The BALLDONTLIE API (an API key in `BALLDONTLIE_API_KEY`). The first sync of a season fetches every game, schedule included. Later syncs fetch only the days from the last played game to today and keep the stored games outside those days (see [Import and refresh data](../runbooks/import-data.md)). | The ratings follow the results. The season is marked `computed`. The import log records the source URL. |
| Results import | A CSV file in the results layout with the played games, and optionally the games that are not played yet. | The ratings follow the results. The season is marked `computed`. |
| Preseason projection | None. | The previous season's regular-season matchups, one year later, with no scores. All ratings are the reverted ratings of the previous season. |

A projection is a placeholder. Its schedule is not the real schedule. Use it to run the [Season forecast engine](season-engine.md) before the real schedule is available, then replace it with a results import.

# Rules

* A results import, a sync or a projection replaces every `computed` game of that season.
* A season that has `imported` games can be completed, not changed. The archive ends with the 2024-25 regular season; a sync of 2024-25 keeps every archive game, adds the play-in and playoff games after the last archive date, and chains their ratings from the ratings after the last archive game. A game is matched to the archive by its date and its two teams, in either order, because sources disagree on the home team at a neutral site (the NBA Cup final). A game dated inside the archive range that is not in the archive stops the sync with a message. A projection of such a season is refused.
* After you complete a season, sync the next season again: its start ratings come from the final ratings of the completed season.
* A later archive import replaces `computed` games of the same key with `imported` games. A `computed` game never replaces an `imported` game.
* The application needs the previous season in the store. It takes the final rating of each team from the last game of that season.
* Team codes must exist in the reference table. Unknown codes stop the import with a message.
* The BALLDONTLIE adapter maps three abbreviations to the dataset's codes: BKN to BRK, CHA to CHO, PHX to PHO. A game counts as played only when its status is `Final`. The adapter paces its requests to the plan's quota and waits when the API answers with status 429.

# Limits

* The computed ratings are the application's own output. They are not FiveThirtyEight ratings.
* The margin-of-victory formula uses the regular-season K-factor of 20 for playoff games too. The archive applies the same K in the playoffs.
* Derived champions and seeds for a computed season follow the same rules as for imported seasons. A season with no playoff results has no derived champion.
