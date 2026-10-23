---
type: Architecture Decision
title: Compute first
description: Why the application computes every Elo rating itself and keeps the archive's ratings only as reference values.
tags: [decision, elo, data]
generated: { by: "human:abenamer", at: "2026-10-23T08:00:00Z" }
---

# ADR 0004 — Compute first

## Context

Until 2026-10-23 the application used the archive's ratings for the seasons it covered (`imported`) and computed its own ratings only for later seasons (`computed`). Two rating chains lived in one table, with rules to keep a computed game from overwriting an imported one and to complete an archive season only after its last archive game. A replay of the application's rating code over the archive showed that it reproduces the archive to a mean difference of 0.007 points; the remaining differences are corrections inside the archive itself.

## Decision

* The application computes every rating itself, season after season from the first season in the store, with the validated formulas. The archive supplies results and schedules; its ratings are stored as reference values in `ref_` columns and used only for validation.
* Each game records its result source (`archive`, `results`, `projection`). Archive games always stand: a results source or a projection never replaces or removes one.
* The start rating of a season follows the franchise, not the team code, so a relocated team continues its lineage. A franchise without a previous season starts at 1300; a franchise whose first archive game carries a reference rating starts there (the clubs that joined from other leagues in 1948 to 1950).
* An archive import, and the first start after this change, compute every season again. A results sync or upload computes its season again from the season's start ratings.

## Consequences

* One computation owns every number on every page. A change in the formulas or in the archive's results reaches every season the same way.
* The archive's own corrections (2007-08, 2015, 2020-21, 2021-22) no longer appear as jumps in the stored ratings; the application carries each game forward consistently. The differences are documented in [Elo continuation](../concepts/elo-continuation.md).
* The integration test `elo_replay_reproduces_the_archive_ratings` guards the formulas against the reference values on every build.
* The migration `0003_compute_first` adds the columns; the first start afterwards takes about 15 seconds more to recompute 80 seasons.
* Every write of ratings is one transaction: the full recompute writes all seasons at once, and a results sync or upload replaces a season's non-archive games and re-rates its archive rows together. A failure leaves the store as it was.

## Alternatives

* Keeping the archive's ratings as the source of truth was rejected: it leaves two rating chains, special rules at the seam, and the archive's internal corrections in the data.
* Recomputing only the seasons after a chosen cut was rejected: every season would need the same code paths anyway, and the cut would be arbitrary.
