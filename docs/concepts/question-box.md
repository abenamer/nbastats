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
---

# Purpose

The question box on the Dashboard answers a question in plain language from the data of this application: "Which champion had the highest peak rating?", "How did the Warriors do in the 2016 playoffs?", "Who won in 1972?". It is a lookup assistant for the database, in the manner of StatMuse, and nothing more.

# How it works

1. The page sends the question to `POST /api/ask`.
2. The use case (`application::ask`) starts a conversation with a language model through the `LanguageModel` port. The concrete adapter is `adapters::openrouter::OpenRouterModel`, which calls OpenRouter's OpenAI-compatible chat API. The system prompt names the data, the scope and the rules.
3. The model does not see the database. It asks for data through seven read-only tools; the use case runs each one against the store ports and returns compact JSON to the model. The tools: `list_seasons`, `season_standings`, `champions`, `franchise_history`, `team_games`, `rating_extremes` and `win_probability`. The probability calculator prices a single game with the current season's regular-season home-court bonus and a series with the playoff bonus of 100; its answer names both.
4. When the model answers in text, the use case returns the answer with the list of lookups it made (tool, arguments, rows), the model name and the token counts. The page shows the answer and the lookups.

The conversation is bounded: one question is at most 6 model calls, 10 tool calls and 75 seconds; calls past the tenth are dropped before they enter the conversation. A tool result longer than 16,000 characters is cut, the results of one question add up to at most 48,000 characters (later calls get a note instead), and the model is told to narrow its request. The model's answer body is read up to 4 MB, and the adapter follows no redirect. The last model call offers no tools, so the model has to answer with what it has. At most two questions run at the same time; a third gets status 429.

# Scope

The box answers questions about the stored data only: teams, seasons, games, standings, playoff results, champions and Elo ratings. Everything else is forbidden: players, coaches, awards, other sports, news, opinions, general knowledge, writing or coding help, and requests to ignore the rules. Two controls enforce this:

* The system prompt forbids every other subject and gives the model one fixed sentence to decline with.
* The use case replaces an answer that made no lookup with that sentence. A lookup is a call to one of the six store tools that returned data; the probability calculator and a call that returned an error do not count. An answer therefore rests on at least one row read from the database, or it is the refusal. After a lookup, the answer's words are the model's: the prompt tells it to use only the tool results, and the lookup list shows what it read. The response marks a refusal with `declined: true`, and the page shows it in grey.

The model can misread a result. Check an important number against the pages; the lookup list under the answer shows where it came from.

# Data that leaves the computer

The question, the system prompt and every tool result the model asks for are sent to OpenRouter and from there to the provider of the chosen model. The data is team-level basketball data from public sources; no personal data is stored in the application. The API key travels in the request header only and is never logged or returned by the API. Leave `OPENROUTER_API_KEY` empty to keep the box off; the panel then says so.

# Enable the box

1. Create a key at OpenRouter and put it in `.env` as `OPENROUTER_API_KEY`.
2. Choose a model with tool calling in `OPENROUTER_MODEL` (default `openai/gpt-4o-mini`; a small, cheap model is enough for lookups). A `:floor` suffix lets OpenRouter pick the cheapest provider of that model. `openrouter/auto` lets OpenRouter pick the model, but a model without tool calling then answers every question with the refusal. `OPENROUTER_BASE_URL` points to another OpenAI-compatible service when needed; the application warns at start when it is not `https`.
3. Restart the application (`docker compose up -d`). The log line `question box enabled` names the model.

The cost of one question is the tokens of the conversation; a typical question with two lookups is a few thousand tokens, and the bounds above hold the worst case near 100,000 input tokens. The token counts appear under each answer. Set a spending limit on the key at OpenRouter: the application has no daily budget of its own, and any local process can post questions.
