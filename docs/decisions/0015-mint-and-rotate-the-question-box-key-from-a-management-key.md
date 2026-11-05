---
type: Architecture Decision
title: Mint and rotate the question box's key from a management key
description: Why the application mints its own OpenRouter completion key from a management key when it starts, replaces it on a schedule and when the service refuses it, deletes the keys it minted before, and keeps no secret of its own.
tags: [decision, language-model, openrouter, security, keys]
generated: { by: "human:abenamer", at: "2026-11-05T09:00:00Z" }
---

# ADR 0015 — Mint and rotate the question box's key from a management key

## Context

The question box ([ADR 0005](0005-question-box-via-openrouter.md)) asks a model through OpenRouter with one API key from the environment. The service disabled the maintainer's key, and the box was off until a new key was made by hand and the application restarted. OpenRouter's management API (its management keys, called provisioning keys in the older documentation) can mint, list, update and delete the account's API keys, which makes a key a thing the application can own for a while instead of a secret kept for months.

## Decision

* A second kind of key, `OPENROUTER_MANAGEMENT_KEY`, turns the rotation on. It is held by one new port, `KeyProvisioner` (mint a named key with a spending limit, list the account's keys, delete one), with one adapter, `OpenRouterKeys`, on `{base}/keys`. The management key cannot ask a model, and the completion key cannot manage keys, so each does one job.
* With the management key set, the application mints a completion key when it starts, named `OPENROUTER_KEY_NAME` (`nba-elo-simulator` by default), a space and the minute in UTC, installs it in the model, and replaces it every `OPENROUTER_KEY_ROTATION_HOURS` (24 by default, 1 to 8,760) and when the service answers a question with status 401, its answer to a disabled or invalid key. A 403 is the service's moderation refusing the question and mints nothing, so a question any local process posts cannot churn the key. Each minted key carries `OPENROUTER_KEY_LIMIT` credits when it is set, so a key that leaks is bounded by the limit. `OPENROUTER_API_KEY` may be set beside it: it answers until the first minted key is installed, and alone it works as before.
* The keys the application minted before are retired: the one replaced, and any named with the prefix and a space left at the account by an earlier run, two minutes after the fresh key is installed, so a question in flight (at most 75 seconds) finishes on the old one. At shutdown the application gives a minting in flight a moment to finish, then deletes its key in use and the ones still retiring; what it could not delete, the next start retires. The account's other keys are never touched: a key is the application's by its name alone, and a key named the bare prefix, as one made by hand under the project's name would be, is not. Two instances sharing one management key set two prefixes, or each retires the other's key.
* Two attempts to mint come at least ten minutes apart, whether the second is forced by a rejected question or retries a failed minting: a key the service refuses for another reason, or a management key the service refuses, cannot make the application mint a key per question. A failed deletion is tried again after another grace, five times, then logged for the maintainer and not taken up again.
* The application keeps no secret of its own: the minted key goes to the model's memory and nowhere else, not to the database or a file, and `GET /api/ask/status` reports times and counts only (`key_rotation`: whether it is on, the interval, the rotations, when the key was minted and when the next one comes, the keys retiring, the kind of the last failure in the application's words, not the service's). The management key is read from the environment, redacted in `Debug` output and never logged or returned.

## Consequences

* A disabled, leaked or expired completion key costs one question: the next one mints a fresh key. A restart mints one too, so the key in the environment can be left empty.
* The management key is the secret that matters now: it can spend the account. It is held in `.env` as before, and its only outbound host is `OPENROUTER_BASE_URL`, with no redirect followed. The security model names the risk.
* The service's limit on a key is per key, so a minted key starts with a fresh limit: the limit bounds a leak, not the month's spending. The account's own limits do that.
* The feature was written from the service's documentation and could not be tried against the service from the development session, whose network did not reach it. The adapter's parsing is tested on the documented answers; the first run against the service is the maintainer's, and the log says what happened.

## Alternatives

* Reading a new key from the environment without a restart was rejected: it still asks the maintainer to notice, make a key and edit a file, and the box stays off meanwhile.
* Storing the minted key in the database so a restart reuses it was rejected: it puts a secret in the store and its backups, for no gain, since minting at start costs one request.
* Disabling the replaced key with `PATCH` instead of deleting it was rejected: a disabled key stays in the account's list for the maintainer to clean up, and nothing reads a disabled key again.
* Rotating only on a rejection, with no schedule, was rejected: a regular rotation limits what a leaked key is worth to the hours until the next one, and costs one request a day.
