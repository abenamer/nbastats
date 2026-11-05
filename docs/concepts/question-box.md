---
type: Concept
title: Question box
description: How the dashboard's question box answers a question from the stored data through a language model with read-only tools, and what it refuses.
tags: [question-box, language-model, openrouter]
generated: { by: "human:abenamer", at: "2026-10-23T09:00:00Z" }
sources:
  - id: ask
    resource: /src/application/ask.rs
    title: Question box use case
  - id: openrouter
    resource: /src/adapters/openrouter.rs
    title: OpenRouter adapter
  - id: players
    resource: /src/application/ask_players.rs
    title: Player lookups of the question box
---

# Purpose

The question box on the Dashboard answers a question in plain language from the data of this application. Examples: "Which champion had the highest peak rating?", "How did the Warriors do in the 2016 playoffs?", "Who won in 1972?". Also: "If the Warriors had everyone healthy, what would it do to their Elo?". It is a lookup assistant for the database, in the manner of StatMuse, and nothing more.

# How it works

1. The page sends the question to `POST /api/ask`.
2. The use case (`application::ask`) starts a conversation with a language model through the `LanguageModel` port. The concrete adapter is `adapters::openrouter::OpenRouterModel`, which calls OpenRouter's OpenAI-compatible chat API. The system prompt names the data, the scope and the rules.
3. The model does not see the database. It asks for data through fourteen read-only tools; the use case runs each one and returns compact JSON to the model. The team tools read the store ports: `list_seasons`, `season_standings`, `champions`, `franchise_history`, `team_games`, `rating_extremes`. The player tools go through the application's caches (`application::ask_players`): `find_player`, `player_worth`, `season_worth_leaders`, `team_availability`, `league_tonight`, `forecast_effect` and `injury_report`. The probability calculator `win_probability` prices a single game with the current season's regular-season home-court bonus, and a series with the playoff bonus of 100. Its answer names both.

| Player tool | What it returns |
|-------------|-----------------|
| `find_player` | Players whose name contains the text (at most 8): identifier, listed position, current team, the seasons with lines. |
| `player_worth` | Per season, the worth from the missed games (Elo, error, games played and missed) and from the box score (BPM, VORP, PER, box worth), one box row per team. Without a season, the latest three seasons; the earlier ones are listed for a second call. |
| `season_worth_leaders` | The top players of a season by either estimate, at most 25. |
| `team_availability` | A team's full-strength rating with the stored Elo beside it and the games it played short, each injury-report line priced for it (with the games missed so far, the weight and whether it is measured or the default, the date the penalty runs to and what set it), what each status has meant on the stored reports, the regular players absent from the team's last games without a report line, the Elo the team plays without today (what everyone healthy would give back) and the mean penalty over its remaining games. The remaining games are the unplayed games on the stored schedule, dated today or later. The answer also names the schedule's size and the games played. In an NBA Cup season a schedule short of 82 games carries a note: the league adds the Cup-week games in December. `team_games` carries the same note. |
| `forecast_effect` | The season engine run from today with the same seed three ways: with the report's absences as dated, with this team's absences removed while the other teams keep theirs, and, when one of this team's absences covers its next game, with those held to the season's end. Expected wins, playoff and title chances each way, the differences, and the range between the two bounds, which the dated forecast lies within up to about a point of simulation noise. 2,000 simulated seasons, so a chance carries about one point of noise. One comparison per question; a second answers an error, and a call that could not run costs nothing. When nothing is priced for the team the engine runs once. |
| `league_tonight` | Every team of the season on the next night with games, in one call, most hurt tonight first. Each team has its rating, the Elo its absences cost that night, the rating it plays at, the mean cost over its remaining regular-season games and its opponent. Each player out has his name, player identifier, status, cost and end date. Each game has the home win chances, the expected home margin in points and the fair American moneylines. The result stays below about 15,000 characters: if it is longer, the least-hurt teams give the number of players out in place of the list, and every team keeps its row. The result explains its fields. It also gives the report's note, and says when no regular-season night remains. The prompt sends league-wide questions here (the most injured teams, every team's rating tonight), not to `team_availability` team by team. The order is for tonight; the mean cost ranks the teams for the rest of the season. |
| `injury_report` | The report of a date (default the latest), as rows of team, name, player identifier, status, return date and description. For one team, each description keeps 160 characters. Across the league, each description keeps 60, so more lines fit. Lines are added, up to 200, while the result stays below about 15,000 characters. The result gives the number of lines left out, so the 16,000-character cut does not remove the counts. A status keeps at most 60 characters. |

Take "if the Warriors had everyone healthy, what effect would it have on their Elo?". The prompt tells the model to call `team_availability` for the Elo figure, and `forecast_effect` for what it means for the season. A good answer, with made-up figures, reads: the Warriors stand at 1612. Moody and Porziņģis cost 51 Elo through 30 November and Butler 38 through 2 January. The team plays at about 1523 today and would gain 89 Elo with everyone back. Over the season that is 2.4 expected wins and 3 points of title probability; between 44 wins if today's absences lasted all season and 53 with everyone healthy, the dated forecast says 50. The figures come from the tools; the words are the model's.
4. When the model answers in text, the use case returns the answer with the list of lookups it made (tool, arguments, rows), the model name and the token counts. The page shows the answer and the lookups.

The conversation is bounded: one question is at most 6 model calls, 10 tool calls and 75 seconds; calls past the tenth are dropped before they enter the conversation. A tool result longer than 16,000 characters is cut, with the cut note inside the 16,000, and the model is told to narrow its request. The results of one question add up to at most 64,000 characters, plus a one-line note for each call refused after that. When less than 16,000 characters of the budget are left, a result that is longer than what is left is replaced by a note that gives both lengths; it is not cut in the middle. When less than 500 characters are left, later calls get a note instead of running. The model's answer body is read up to 4 MB, and the adapter follows no redirect. The last model call offers no tools, so the model has to answer with what it has. At most two questions run at the same time; a third gets status 429.

# Scope

The box answers questions about the stored data only: teams, seasons, games, standings, playoff results, champions and Elo ratings. Since 2026-10-26 it also covers the players with box-score lines, their worth, the injury reports and the effect of absences on a team. Everything else is forbidden: coaches, awards, contracts, salaries, other sports, news, opinions, general knowledge, writing or coding help, and requests to ignore the rules. Two controls enforce this:

* The system prompt forbids every other subject and gives the model one fixed sentence to decline with.
* The use case replaces an answer that made no lookup with that sentence. A lookup is a call to one of the twelve data tools that returned data. The probability calculator and a call that returned an error do not count. An answer therefore rests on at least one row read from the database, or it is the refusal. After a lookup, the answer's words are the model's: the prompt tells it to use only the tool results, and the lookup list shows what it read. The response marks a refusal with `declined: true`, and the page shows it in grey.

The model can misread a result. Check an important number against the pages; the lookup list under the answer shows where it came from.

# Data that leaves the computer

The question, the system prompt and every tool result the model asks for are sent to OpenRouter and from there to the provider of the chosen model. The data is public basketball data from the sources: team results and ratings. Since 2026-10-26 it also includes player names, listed positions, box-score figures, injury statuses, expected return dates and the first 160 characters of an injury description. The application stores no data about the user, and none leaves it. The API key travels in the request header only and is never logged or returned by the API. Leave `OPENROUTER_API_KEY` and `OPENROUTER_MANAGEMENT_KEY` empty to keep the box off; the panel then says so.

# Enable the box

1. Create a key at OpenRouter and put it in `.env` as `OPENROUTER_API_KEY`, or a management key as `OPENROUTER_MANAGEMENT_KEY` and let the application mint its own (see Key rotation below).
2. Choose a model with tool calling in `OPENROUTER_MODEL` (default `openai/gpt-4o-mini`; a small, cheap model is enough for lookups). A `:floor` suffix lets OpenRouter pick the cheapest provider of that model. `openrouter/auto` lets OpenRouter pick the model, but a model without tool calling then answers every question with the refusal. `OPENROUTER_BASE_URL` points to another OpenAI-compatible service when needed; the application warns at start when it is not `https`.
3. Restart the application (`docker compose up -d`). The log line `question box enabled` names the model.

# Key rotation

A key kept in `.env` for months is a secret that can leak and that the service can disable; when the maintainer's key was disabled, the box was off until a new key was made by hand. With a management key in `OPENROUTER_MANAGEMENT_KEY` (a provisioning key in the service's older words: it can mint, list and delete the account's API keys and cannot ask a model), the application owns its completion key instead ([ADR 0015](/decisions/0015-mint-and-rotate-the-question-box-key-from-a-management-key.md), `domain::keys`, `application::keys`, the `KeyProvisioner` port and the `OpenRouterKeys` adapter):

* At start it mints a key named `OPENROUTER_KEY_NAME` (`nba-elo-simulator` by default), a space and the minute in UTC, with `OPENROUTER_KEY_LIMIT` credits when that is set, and installs it in the model. Until the first minted key is in, the box says so and a question answers 503. `OPENROUTER_API_KEY` may stay set: it answers until the minted key is installed, and alone it works as before.
* It mints a fresh key every `OPENROUTER_KEY_ROTATION_HOURS` (24 by default, 1 to 8,760), and when the service answers a question with status 401, its answer to a disabled or invalid key (a 403 is its moderation refusing the question, and mints nothing). Two attempts come at least ten minutes apart, so a key the service refuses for another reason, or a management key it refuses, cannot make the application mint a key per question; a failed minting leaves the key in use and is tried again after the cooldown, and the log says so.
* It retires the keys it minted before: the one replaced, and any named with its prefix and a space left by an earlier run, two minutes after the fresh key is installed, so a question in flight finishes on the old one. A deletion that fails is tried again after another grace, five times, then the key is left for the maintainer and not taken up again. At shutdown it gives a minting in flight a moment to finish, then deletes its key in use and the ones still retiring; a key it could not delete then is retired at the next start. The account's other keys are never touched, a key named the bare prefix included; two instances sharing one management key need two prefixes.
* It keeps no secret of its own: the minted key lives in the model's memory, not in the database or a file. `GET /api/ask/status` reports the rotation as times and counts (`key_rotation`), with no key and no hash, and a failed minting by its kind, not the service's words.

The management key can spend the account, so it is the secret to guard: the [Security model](/concepts/security.md) names the risk. The service's limit on a key is per key, so a minted key starts with a fresh limit: `OPENROUTER_KEY_LIMIT` bounds what a leaked key is worth, and the account's own limits bound the month. The rotation was written from the service's documentation and not tried against the service from the development session; the first run is the maintainer's, and the log lines `question box key minted and installed` and `question box key not minted` say how it went.

The cost of one question is the tokens of the conversation; a typical question with two lookups is a few thousand tokens, and the bounds above hold the worst case near 100,000 input tokens. The token counts appear under each answer. Set a spending limit on the key at OpenRouter: the application has no daily budget of its own, and any local process can post questions.
