---
type: Technical Reference
title: Project terminology
description: Project-specific technical terms used in this bundle and in the code.
tags: [terminology, ste]
generated: { by: "human:abenamer", at: "2026-10-21T19:00:00Z" }
sources:
  - id: reas-terms
    resource: https://github.com/abenamer/reas/blob/main/standard/terminology.md
    title: REAS terminology
---

# Project terminology

These are project-specific technical terms. They are not a reproduction of the ASD-STE100 dictionary. The [REAS terminology](https://github.com/abenamer/reas/blob/main/standard/terminology.md) defines the general Rust and architecture terms (port, adapter, newtype, typestate, blanket implementation).

| Term | Meaning |
| --- | --- |
| Alpha | Strength weight of a franchise in Engine A. The title probability is alpha divided by the sum of all alphas. |
| Bracket | The 16 playoff slots and their pairings. Slots 0 to 7 are the East, 8 to 15 the West. |
| Composition root | `src/main.rs`. The one place that creates the concrete adapters and wires them to the application. |
| Cup final | The NBA Cup (in-season tournament) final. An extra game that does not count in the standings. |
| Elo | A rating that gives game probabilities from the rating difference. |
| Elo continuation | The computation of ratings by the application for seasons after the imported archive ends. |
| Elo source | The origin of a game's ratings: `imported` from an archive file or `computed` by the application. |
| Engine A | The alpha lottery simulator (`domain::simulation::alpha`). |
| Engine B | The playoff bracket simulator (`domain::playoffs::bracket`). |
| Engine C | The full-season simulator with play-in (`domain::playoffs::season`). |
| Franchise | The organisation, followed through relocations and renamings. Identified by `FranchiseId` and a key such as `GSW`. |
| Game log | The imported list of games with Elo before and after each game. |
| Information cutoff | The last date whose games a forecast may use. |
| Job | A stored simulation request with parameters, seed, versions, progress and result. |
| Play-in | The games that decide seeds 7 and 8 (2020-21 and later), or the 8 seed (2019-20). |
| Port | A trait in `application::ports` that the application requires from the outside world. |
| Projection | A placeholder season: the previous season's matchups one year later, no scores, reverted ratings. |
| Scenario | A named, completed job that you can reopen and compare. |
| Season | An NBA season named by its end year. `SeasonYear(2004)` is 2003-04. |
| Seed | Playoff position 1 to 8 inside a conference. |
| Series | A best-of-five or best-of-seven playoff matchup. A `SeriesState` is either in progress or decided. |
| Store | The `PostgresStore` adapter, which implements every store port. |
| Team | A team code and era, such as SEA or OKC. Identified by `TeamId`. |
