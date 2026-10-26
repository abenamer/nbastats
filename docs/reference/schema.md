---
type: Reference
title: Database schema
description: Tables, keys and derived data in PostgreSQL.
tags: [database, schema, postgresql]
generated: { by: "human:abenamer", at: "2026-10-21T17:30:00Z" }
sources:
  - id: migration
    resource: /migrations/0001_initial_schema.sql
    title: Initial migration
  - id: players
    resource: /migrations/0005_players.sql
    title: Player tables
  - id: rosters
    resource: /migrations/0006_player_rosters.sql
    title: Rosters and plus/minus
---

# Reference tables

| Table | Key | Content |
|-------|-----|---------|
| `franchises` | `key` | One row per franchise, for example `GSW`. |
| `teams` | `code` | One row per team code and era. Links to a franchise. |
| `alignments` | (season, team) | Conference and division per season, from 2002-03. |

# Imported data

| Table | Key | Content |
|-------|-----|---------|
| `games` | (date, home, away, seq) | One row per game: scores, Elo before and after, source forecast, type, Elo source (`imported` or `computed`). |
| `data_imports` | id | Source, file checksum, row counts, status, timestamps. |

# Derived data

The import rebuilds these tables for every season it touches.

| Table | Key | Content |
|-------|-----|---------|
| `seasons` | season | Counts (including computed and scheduled games), dates, completeness. |
| `team_seasons` | (season, team) | Record, Elo milestones, seed, playoff result. |
| `championships` | season | Champion, runner-up, source (`derived` or `curated`). |

# Simulation data

| Table | Key | Content |
|-------|-----|---------|
| `simulation_jobs` | id (UUID) | Kind, name, status, parameters (JSONB), result (JSONB), seed, versions, timestamps. |
| `forecasts` | (job, model, season, team) | Backtest forecasts with the information cutoff date. |

# Player data

Added by migration `0005_players`; see [Player data](/concepts/player-data.md).

| Table | Key | Content |
|-------|-----|---------|
| `players` | (source, source_id) | Name, position, height, weight, college, country, draft year, round and pick; `jersey_number`, `team_id` (current team) and `active` from migration `0006`; `updated_at`. |
| `appearances` | (game, player) | One player's line in one game: team, seconds, points, rebounds (total, offensive, defensive), assists, steals, blocks, turnovers, fouls, field goals, three-pointers and free throws made and attempted, `plus_minus` (migration `0006`). Deleted with the game. |
| `box_score_fetches` | game date | Source, time of the fetch, lines received, matched and unmatched. |
| `injury_reports` | (report date, player) | Team, status, expected return date, description, time of the fetch. |

# Indexes

Games are indexed by (season, date), by (season, Elo source), by home team and by away team. Jobs are indexed by status and by kind. The parameters column has a GIN index for containment queries. Appearances are indexed by player and by team; injury reports by player; players by current team.
