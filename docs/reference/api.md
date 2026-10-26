---
type: API
title: HTTP API
description: Pages and JSON endpoints served on port 3000.
tags: [api, http, json]
generated: { by: "human:abenamer", at: "2026-10-21T17:30:00Z" }
sources:
  - id: router
    resource: /src/adapters/http/mod.rs
    title: Router
---

# Pages

`/`, `/elo`, `/alpha`, `/playoffs`, `/season`, `/dynasty`, `/validation`, `/data`, `/jobs`. See the [application overview](/concepts/application.md).

# Health

| Method and path | Function |
|-----------------|----------|
| `GET /healthz` | 200 when the database answers, else 503. |
| `GET /api/health` | Version, uptime, database health, table counts, job counts. |

# Question box

| Method and path | Function |
|-----------------|----------|
| `GET /api/ask/status` | `enabled`, the configured `model` and `max_question_chars`. |
| `POST /api/ask` | JSON body `{"question": "..."}` (at most 500 characters, body limit 16 KB). Answers from the stored data through the language model's read-only tools: `answer`, `declined` (true when the question is outside the data; `answer` is then the fixed sentence), `steps` (each lookup with tool, arguments and a row count), `model`, `rounds`, `input_tokens`, `output_tokens`. Status 503 when `OPENROUTER_API_KEY` is not set, 429 when two questions are already running, 502 when the model service fails or does not answer in 75 seconds. See the [question box](/concepts/question-box.md). |

# Data

| Method and path | Function |
|-----------------|----------|
| `GET /api/seasons` | Season list with counts and dates. |
| `GET /api/seasons/{season}/teams` | Team-season rows: record, Elo milestones, seed, result. |
| `GET /api/seasons/{season}/elo-history?teams=GSW,CLE` | Elo after each game, and the `schedule` span (first and last game, last played game, games remaining). |
| `GET /api/seasons/{season}/bracket?home_advantage=100&scale=1` | Reconstructed bracket with exact probabilities. |
| `GET /api/seasons/{season}/player-worth` | Every player's worth in the season, highest first: `worth_elo` and `worth_points` with `se_elo` and `se_points`, `games_played`, `absences`, the raw `with_mean` and `without_mean`, the player's `name` and `team_codes`; the table's `prior_sd_points` (and whether `prior_estimated`), `residual_sd_points`, `games`, `games_with_absence`, `players_seen` and `elo_per_point`. Status 404 for a season without games. See [Player worth](/concepts/player-worth.md). |
| `GET /api/seasons/{season}/availability` | The latest injury report priced for the season: `report_date`, `season_end`, each line with its status, return date, the worth used and its season, and the `penalty_elo` with `until` when one applies; `absences` is the list a forecast takes, and `note` says why it is empty when it is (no report stored, or a report outside the season). |
| `GET /api/players/{id}/worth` | The player and his worth in every season he has one. Status 404 for an unknown identifier. |
| `GET /api/seasons/{season}/home-advantage` | The home-court bonus the season's forecasts use: `regular` (fitted on the regular-season games of the previous five seasons, at least 2,000, within -100 to 300; else 100), `playoff` (100), `fitted_from` (the first and last season of the fit, null when the default stands) and `games` (the games of the window). See the [Elo rating model](/concepts/elo-model.md). |
| `GET /api/records/groups?by=cba,rule&league=nba&from=1947&to=2025&dynasty_titles=3&dynasty_gap=3&facet=player:LeBron%20James` | Group panel of the Records page: one group per combination of the `by` dimensions (`cba`, `rule`, `size`, `league`, `decade`, `format`, `dynasty`, `player`, `theme`; at most four), within the `facet` seasons when any are given, with its seasons, records (best, weakest, titles, champions, distinct champions, `title_hhi`, `mean_elo`, `elo_spread`, `champion_z`) and the notes of its periods; also the dynasties found and the `scope`. `league` is `nba`, `aba` or `all`; a scalar parameter given twice keeps its last value; the dynasty thresholds shape the group rows, the dynasty facets use the defaults. See the [era builder](/concepts/era-builder.md). |
| `GET /api/records/facets` | The facets the data offers (no parameters): every dimension with its labels, each with its first and last season, its season count and a summary when its period or player has one; `max_facets` is the number a query may carry. |
| `GET /api/reports?dates=2019-03-23,2010-03-13` | Game report links for the played games of the dates (at most 40): `reports` per date with `home_code`, `away_code`, `url`, `title`, `source`; `pending` lists dates not looked up yet (ask again in a moment); `disabled` is true when lookups are off. The first request for a date asks the report source, at most six dates within fifteen seconds per request. |
| `GET /api/records?facet=player:LeBron%20James&facet=rule:Hand-check%20ban` | Every record table of the Records page, computed from the game log on each call; with `facet` parameters (repeated, `dimension:label`, at most eight; within a dimension any, across dimensions all), the same tables over the selected seasons only, and `scope` names the facets, the seasons, the leagues, the records of the selection (`stats`) and the notes of the periods picked (null without facets); other parameters are ignored. `by_cba` holds the records of each collective bargaining period (seasons, best and weakest team-season, titles and champions, `title_hhi`, `mean_elo`, `elo_spread`, `champion_z`, `changes`). `parity` holds one row per season and league with at least eight rated teams (league mean and spread, the champion with its rank, and the champion, finalists, final four and playoff field tiers, each with its teams, mean, `gap`, `z` and `gap_to_rest`; a tier the round labels do not complete is null), and `parity_summary` the mean z of each tier over those seasons, with `seasons_with_champion` as the denominator of `champion_was_best`; see the [parity profile](/concepts/parity.md). A label no season carries, or a facet not written `dimension:label`, answers 400. |
| `GET /api/franchises` | Franchise list with uniform colours (`colors.primary`, `colors.secondary`; defunct franchises get a neutral pair). |
| `GET /api/franchises/{key}/elo?from=2003&to=2025` | Elo after every game of a franchise, its season rows, its championships (won and lost) and its colours. |
| `GET /api/matchup?elo_a=1700&elo_b=1600&home=a` | Game and series probabilities; `home_advantage` is optional and defaults to 100. |
| `GET /api/exact/alpha?franchises=30&seasons=22&threshold=4` | Exact equal-strength occupancy probabilities. |
| `GET /api/strength?franchise=GSW&from=2015&to=2022&titles=4` | The strength a title record implies, computed exactly. `alpha`: the title chance per season (`probability`, with its Wilson 95 percent interval), the alpha against every other franchise at 1 (`alpha`, null for a title every season, with `alpha_low` and `alpha_high`), the chance of at least that many titles at that strength (`p_at_least`) and for an ordinary franchise (`p_at_least_equal`), and the chance and alpha at which the record becomes a coin flip (`even_odds_probability`, `even_odds_alpha`). `elo`: against the actual pre-playoff fields of the window's seasons (the franchise's own ratings left out), the gap above the league mean at which the title chance equals the record's (`implied`, with `low`, `high` and `even_odds`, each as `gap`, `rating`, `points_per_game`, `game_win_probability` and `expected_wins_82`), each reading also carrying the `title_chance` the gap really has, which differs from the chance asked for where the team changes seed), the `seasons` with a field, the `league_mean`, the `home_advantage` and the title chance of an average team (`p_average`); null when no season of the window has at least 16 rated NBA teams. `actual`: the franchise's mean pre-playoff gap over the window and the best team's. The alpha reading counts the franchises of the last season with a field (30 without one). `franchise` is optional and must name a franchise in the store; `from` and `to` are season end years spanning at most 40 seasons (a long window answers in a few seconds); `titles` is 1 to the number of seasons. One reading runs at a time; a second answers 429. |
| `GET /api/dashboard?from=2004&to=2025` | Title distribution and concentration. |
| `GET /api/dynasty?franchise=GSW&from=2015&to=2022&alpha=3` | Dynasty report and concentration comparison. |
| `GET /api/data/imports` | Import history. |
| `GET /api/data/coverage` | Season coverage (per season: game counts, `archive_games`, `computed_games`, `scheduled_games`, `removable`), missing seasons, the next season without data, an example of the results layout, the configured results source (`null` when none) and the daily sync schedule with its next run (`null` when disabled). |
| `POST /api/data/refresh` | Download and import the source. Body: `{"force": false}`. |
| `POST /api/data/upload` | Multipart upload. Fields: `file`, `force`. |
| `POST /api/data/project` | Preseason projection. Body: `{"season": 2026}`. Refused when the season has archive games. |
| `POST /api/data/results` | Multipart upload in the results layout. Fields: `file`, `season`. Archive games of the season stand; the season's ratings are computed again. |
| `DELETE /api/data/seasons/{season}` | Remove a season without archive games. Status 400 for a season with archive games. |
| `POST /api/data/sync` | Start a background sync of a season from BALLDONTLIE; archive games stand and only the games the archive does not have are added. Body: `{"season": 2026}`. Returns at once. Status 400 when `BALLDONTLIE_API_KEY` is not set, 409 when a sync is already running. |
| `GET /api/data/sync/status` | State of the last or current sync: `idle`, `fetching` (season, trigger, requests, games, seconds until the next request), `done` (outcome) or `failed` (error). A synced game whose Cup stage is the final is stored as a Cup final. |
| `GET /api/data/players?season=2026` | Player data coverage. `coverage` holds the players, `active_players`, lines, dates fetched with the first and last, and injury-report days with the latest. For the season given, `fetches` lists each date with its time, lines, matched, unmatched and players, and `dates_missing` counts the played dates without a fetch. `backfill` lists every season with a played game, with its `dates_total` and `dates_fetched`. `backfill_from` is the season the background loop starts from (null when off). `configured` says a results source exists, `daily_sync` that the daily player sync is on, and `settling_days` the window. See [Player data](/concepts/player-data.md). |
| `GET /api/data/players/status` | State of the last or current player sync. `idle`. `fetching`: kind, trigger, dates total and done, current date, lines, requests, seconds until the next request. For a run over several seasons it also holds the season with the seasons done and total. `done`: the outcome (dates fetched, lines, matched, unmatched, players, `injury_report`, `players_loaded`, `active_players`). `failed`: the error and the outcome stored before the failure. |
| `POST /api/data/players/backfill` | Start a background fetch of the box scores of every unfetched date of a season. Body: `{"season": 2026}`. Status 400 when `BALLDONTLIE_API_KEY` is not set, 409 when a player sync is already running. |
| `POST /api/data/players/backfill-all` | Start a background fetch over every season from `from` to the current season, newest first, skipping complete seasons. Body: `{"from": 1947}`; without `from`, the earliest stored season. Status 400 for a `from` after the current season, and 400 and 409 as above. |
| `POST /api/data/players/injuries` | Start a background fetch of today's injury report. No body. Status 400 and 409 as above. |
| `POST /api/data/players/load` | Start a background load of the source's whole player list (every player, active or not). No body. Status 400 and 409 as above. |
| `POST /api/data/players/rosters` | Start a background refresh of the current rosters: the listed players become active, the others inactive. No body. Status 400 and 409 as above. |

# Players

| Method and path | Function |
|-----------------|----------|
| `GET /api/players?q=curry` | Players whose name contains `q` (case-insensitive, at least two characters, at most 80), at most 25, by last name. Status 400 for a shorter query. |
| `GET /api/players/{id}` | One player (with `jersey_number`, the current `team_code` and `active`) and his `seasons`: for each season and team, the games, the playoff games, the seconds and the points. Status 404 for an unknown identifier. |
| `GET /api/players/{id}/games?season=2026` | The player's box-score lines in the season, in game order, each with the game's date, type, team, opponent, side and the full stat line (`plus_minus` included). Without `season`, the latest season stored for the player. |
| `GET /api/games/{id}/box-score` | Every stored line of one game: the home team's lines first, each team by seconds played. |
| `GET /api/injuries?date=2026-01-15` | The injury report stored for the date: player, team, status, expected return, description. Without `date`, the latest report stored; `date` is null when none is. |

# Jobs

| Method and path | Function |
|-----------------|----------|
| `POST /api/jobs/alpha` | Queue an Engine A run. Body: `AlphaParams` (`franchises`, `alphas`, `seasons`, `iterations`, `threshold`, `seed`, `target_index` defaulting to 0 for the franchise of interest, and an optional `schedule`, a seasons × franchises alpha table that replaces `alphas` season by season) plus optional `name`. |
| `POST /api/jobs/playoff` | Queue an Engine B run. Body: `PlayoffJob` plus optional `name`. |
| `POST /api/jobs/season` | Queue an Engine C run. Body: `SeasonParams` plus optional `name`. The result carries, per team, `seed_distribution` (the chance of each playoff seed, 1 to 8) beside `position_distribution`, and `projected_bracket` (the most likely bracket seed by seed, with `exact` stage probabilities per slot at fixed ratings, `first_round` series as `slot_a`, `slot_b`, `p_a`, and the counts `exact_count`, `field_count` and `most_common_count` over the `sampled` seasons, the first 200,000, with their shares `p_exact`, `p_field` and `p_most_common`; null when no simulated season reached the playoffs; absent in results stored under model version 1.0). For a season with an NBA Cup, `cup` carries the groups with their tables so far, each team's chances (`p_knockout`, `p_semifinal`, `p_final`, `p_winner`), the projected knockout bracket (`slots`, `exact` chances per slot, `quarterfinals`, and the counts over the sampled seasons) and `actual_winner`; otherwise `cup` is null and `cup_note` says why (model version `elo-season-1.2`; earlier results have neither). See the [NBA Cup forecast](/concepts/nba-cup.md). `home_advantage` is the playoff bonus; `regular_home_advantage` (optional, -100 to 400) prices the regular-season and play-in games and takes the season's fitted value when absent, which the stored parameters then show; status 500 when the store cannot be read.  With `"use_injury_report": true` and no `absences`, the request's `absences` are filled from the latest injury report (see [Player worth](/concepts/player-worth.md)); `absences` given directly (`team_id`, `from`, `to`, `elo`, optional `player_id`) take precedence over the flag, at most 500, each at most 400 Elo.|
| `POST /api/jobs/backtest` | Queue a backtest. Body: `BacktestParams` plus optional `name`; `tuning.criterion` is `series` (default) or `champion`. Each model's report carries `series` (one row per played series: round, winner, loser, games, the winner's probability, Brier, log loss, whether the favourite won), `series_summary` (means with standard errors, the favourite's share and mean probability, the skill against the equal model, and per round the series, the means, the favourite's share and mean probability), `series_vs_first` (the paired difference against the first model) and the favourite's calibration under `calibration.series`; each season row carries its series totals; `tuning_criterion` names what the grid minimised and `in_sample_best` is its mean per trial. A model with a K-factor answers 400. See [Backtesting method](/concepts/backtesting.md). |
| `GET /api/jobs?kind=alpha&limit=50` | List jobs without results. |
| `GET /api/jobs/{id}` | One job with its result. |
| `POST /api/jobs/{id}/retry` | Re-queue a failed, interrupted or cancelled job. |
| `POST /api/jobs/{id}/cancel` | Cancel a queued or running job. |
| `POST /api/jobs/{id}/name` | Rename. Body: `{"name": "..."}`. |
| `DELETE /api/jobs/{id}` | Delete a job that is not running. |
| `GET /api/scenarios/{kind}` | Named, completed jobs of a kind. |
| `GET /api/backtests/latest` | The latest completed backtest. |

# Example

    curl -s -X POST http://localhost:3000/api/jobs/alpha \
      -H 'Content-Type: application/json' \
      -d '{"name":"equal","franchises":["A","B","C"],"alphas":[1,1,1],"seasons":10,"iterations":100000,"threshold":3,"seed":1,"target_index":0}'

Poll `GET /api/jobs/{id}` until `status` is `completed`.

# Errors

Errors return JSON `{"error": "message"}` with status 400 (bad input), 403 (cross-site request), 404 (not found), 409 (conflict), 421 (host not allowed), 429 (too many questions at once), 502 (the language model service failed), 503 (database unavailable, or the question box not configured) or 500 (internal, details only in the log).
