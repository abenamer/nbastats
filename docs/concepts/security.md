---
type: Concept
title: Security model
description: Threat model of a locally hosted, unauthenticated application and the controls in place.
tags: [security, threat-model]
generated: { by: "human:abenamer", at: "2026-10-21T20:00:00Z" }
sources:
  - id: guard
    resource: /src/adapters/http/guard.rs
    title: Browser request guard
  - id: reas-test
    resource: https://github.com/abenamer/reas/blob/main/standard/validation.md
    title: REAS validation rules (REAS-TEST-003)
---

# Assumptions

* The application runs on one computer for one user. It has no accounts and no authentication.
* The dashboard port binds to `127.0.0.1` by default. The database port is not published.
* The browser that opens the dashboard also opens other websites. Those websites must not be able to use the API.

# Threats and controls

| Threat | Control |
|--------|---------|
| A website you visit sends requests to `http://localhost:3000` (cross-site request forgery) | The request guard rejects state-changing requests that the browser marks as cross-site (`Sec-Fetch-Site`) or whose `Origin` differs from the `Host`. Reads stay open. Scripts without browser headers (curl) are not affected. |
| A website uses a host name that resolves to your computer (DNS rebinding) | The `Host` header must match `ALLOWED_HOSTS` (default: localhost, 127.0.0.1, [::1]). Other names get status 421. |
| Script injection through stored names (job names, franchise labels, CSV codes) | Askama escapes template output. The page scripts escape every string before it enters HTML. The Content-Security-Policy allows scripts from the application origin only. |
| SQL injection | Every value is a bound parameter. Only constant column lists and table names are formatted into SQL. |
| Decompression bomb in an uploaded gzip file | Decompression stops at 512 MB and the upload is rejected. The request body limit is 64 MB. |
| Oversized or endless download from the data source | The download stops at 256 MB; the connection has a 5-minute timeout. The source URL is configuration, not user input. |
| CPU exhaustion through simulation parameters | Each job has an iteration limit (`MAX_ITERATIONS`) and an estimated work budget (`MAX_WORK_UNITS`). Parameters outside the limits return status 400 before a job is created. The backward reading of a title record (`GET /api/strength`) runs its exact brackets on a blocking thread, one reading at a time (a second answers 429), over a window of at most 40 seasons. |
| Credential leakage | The database password and the BALLDONTLIE and OpenRouter API keys are read from the environment. The password is masked in logs and on the Data page; the keys are never logged or returned by the API. `.env` is excluded from version control and from the image. |
| Data sent to a third party by the question box | The question, the system prompt and the tool results go to OpenRouter and the model's provider. The data is public team-level basketball data; the application stores no personal data. The box is off until `OPENROUTER_API_KEY` is set, and the panel says what leaves the computer. |
| Prompt injection through a question or a tool result | The model has read-only tools and no SQL; it cannot change data. The use case decides what runs: tool names and arguments are checked, unknown tools and bad arguments return an error object. An answer without a lookup that returned data from the store is replaced by a fixed refusal, so a question cannot make the model answer with no data read; after a lookup, the prompt holds the model to the tool results and the lookup list shows what it read. |
| Cost or time exhaustion through the question box | One question is at most 6 model calls, 10 tool calls (calls past the budget are dropped), 700 answer tokens per call and 75 seconds; a tool result is cut at 16,000 characters and the results of one question at 48,000; the model's answer body is read up to 4 MB; at most 2 questions run at once (status 429); the question is at most 500 characters and the request body 16 KB. There is no daily budget: set a spending limit on the key at OpenRouter. |
| Data sent to ESPN by the game report lookups | One request per date to ESPN's public scoreboard feed, carrying the date only; no key, no personal data. The links kept are `https` links on an ESPN host only, opened in a new tab with `rel="noopener"`. One lookup runs at a time and a request fetches at most six dates within fifteen seconds; a date the feed lists games for is recorded, with or without reports, and asked again only while its games are recent. `GAME_REPORTS_ENABLED=false` makes no such requests. |
| Key sent to the wrong host | The adapter follows no redirect, so the key and the conversation go to `OPENROUTER_BASE_URL` only; the application warns at start when that URL is not `https`. Provider error messages are cut to 300 characters before they reach the log. |
| Oversized or endless response from the results API | Each page stops at 8 MB; a season stops at 200 pages; each request has a 60-second timeout. The base URL is configuration, not user input. The box-score and injury fetches have the same bounds per date and per report. |
| Data sent to BALLDONTLIE by the player sync | The key and a date, or the key alone for the team list and the injury report. No personal data of the user leaves the computer. The data received is public player data (names, positions, lines, injury statuses). The application stores what the source sends, cut at 500 characters per description. One player sync runs at a time. `PLAYER_SYNC_ENABLED=false` stops the daily fetches; the buttons and the command still fetch on request. |
| Player search patterns | The name query is a bound parameter; its `%`, `_` and `\` are escaped so they match themselves. The query is cut at 80 characters and the result at 25 rows. |
| Container escape or privilege escalation | The application runs as a system user without privileges, with a read-only root file system, all capabilities dropped and `no-new-privileges`. |
| Path traversal through file names | Upload file names are reduced to safe characters and used for provenance only. Download file names are generated by the application. Backup labels accept only letters, digits, `_` and `-`. |
| Vulnerable dependencies | `cargo audit` runs in CI and before a release. |

# Headers

Every response carries `Content-Security-Policy`, `X-Content-Type-Options: nosniff`, `X-Frame-Options: DENY` and `Referrer-Policy: same-origin`.

# Residual risk

* Anyone with access to the computer, or to the port when you publish it on a network, can use the full API. If you publish the port, put a reverse proxy with authentication in front of it and set `ALLOWED_HOSTS`.
* There is no rate limit. A local process can queue many jobs; the runner executes them one at a time.
* The `/api/health` page shows table counts, the data directory and the masked database URL.
