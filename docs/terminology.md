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
| Bargaining period | A run of NBA/BAA seasons under one collective bargaining agreement, from the first season the agreement applied to. The table of periods and their changes is curated in `domain::cba`; see [Rule periods and bargaining periods](/concepts/periods.md). |
| Bracket | The 16 playoff slots and their pairings. Slots 0 to 7 are the East, 8 to 15 the West. |
| Competition format | A period of the postseason and in-season structure: early playoffs, the 16-team field, best-of-seven throughout, the play-in era, the NBA Cup era. Curated in `domain::eras`. |
| Composition root | `src/main.rs`. The one place that creates the concrete adapters and wires them to the application. |
| Cup final | The NBA Cup (in-season tournament) final. An extra game that does not count in the standings. |
| Current season | The season the daily sync targets: ends next year from September to December, ends this year from January to August. |
| Decade | Ten seasons grouped by the start year of the season: 1959-60 belongs to the 1950s. |
| Dimension | A way to label seasons on the Records page: bargaining period, rule period, league size, league, decade, competition format, dynasty era, player era or themed era. A facet picks one label; a group query combines dimensions. |
| Dynasty era | A franchise's run of titles, at least a given number with no gap longer than a given number of seasons, from its first title to its last; computed from the champions. |
| Elo | A rating that gives game probabilities from the rating difference. |
| Elo continuation | The computation of every rating by the application, season after season from the first season in the store. |
| Elo source | The origin of a game's ratings. Since compute first every game is `computed`; `imported` marks rows from an older version that the application recomputes at start. |
| Elo spread | The standard deviation of season-end ratings across a league in a season, averaged over a group's seasons. Wider means a more unequal league. |
| Engine A | The alpha lottery simulator (`domain::simulation::alpha`). |
| Engine B | The playoff bracket simulator (`domain::playoffs::bracket`). |
| Engine C | The full-season simulator with play-in (`domain::playoffs::season`). |
| Era | Consecutive seasons of one league with the same number of teams inside one rule period. |
| Era builder | The Records page's facets and group panel: the facets replay every table over the seasons of the picked eras (`GET /api/records?facet=`), the group panel computes one row of records per combination of dimensions (`GET /api/records/groups`). |
| Facet | One label along a dimension, written `dimension:label`, that keeps the seasons carrying it. Within one dimension facets add up, across dimensions they intersect. |
| Fetch window | The dates a results sync asks the source for: the whole season on the first sync, then the days from the last played game to today. |
| Franchise | The organisation, followed through relocations and renamings. Identified by `FranchiseId` and a key such as `GSW`. |
| Game log | The stored list of games with the application's Elo before and after each game, and the archive's reference ratings for archive games. |
| Game report | The wire-service recap of a played game (the Associated Press recap that ESPN hosts), linked from the game's score when the lookup finds one; stored by date and team codes. |
| Information cutoff | The last date whose games a forecast may use. |
| Job | A stored simulation request with parameters, seed, versions, progress and result. |
| Language model | A chat model with tool calling behind the `LanguageModel` port; the `OpenRouterModel` adapter calls it through OpenRouter's OpenAI-compatible API. |
| League size | The number of teams of one league (NBA/BAA or ABA) in one season. |
| Lookup | One tool call the language model makes to read the store (tool, arguments, rows). The question box lists the lookups under each answer. |
| Matchup (greatest, clanker) | A played game ranked by the combined Elo of the two teams before tip-off: the 20 highest are the greatest matchups; the lowest of each era, per league, is its clanker. |
| Mean Elo | The mean season-end rating across a league in a season, averaged over a group's seasons. A check that the ratings hold their level between periods. |
| Play-in | The games that decide seeds 7 and 8 (2020-21 and later), or the 8 seed (2019-20). |
| Player era | The seasons a listed player was active, from the curated table in `domain::players`; a season belongs to every listed player active in it. |
| Port | A trait in `application::ports` that the application requires from the outside world. |
| Projection | A placeholder season: the previous season's matchups one year later, no scores, reverted ratings. |
| Question box | The panel on the Dashboard that answers a plain-language question from the stored data through a language model with read-only tools, and declines every other question with a fixed sentence. |
| Reference rating | The archive's rating before or after an archive game, kept beside the application's rating for validation. |
| Result source | Where a game's result came from: `archive`, `results` (a results source or upload) or `projection`. Archive games always stand. |
| Results source | The `ResultsSource` port: game results and schedule of a season from the BALLDONTLIE API. The adapter is `BalldontlieSource`. |
| Road to the title | A champion's mean opponent rating before each game, shown for the regular season and for the playoffs (play-in included), and combined as the mean over every game, so each phase weighs by its games. Each era shows its champion with the highest road (hardest) and the one with the lowest (easiest). |
| Rule period | A group of eras that starts with a rule change: for the NBA the BAA's founding (pre shot clock), the 24-second shot clock (1954-55), the 3-point line (1979-80), zone defense (2001-02) and the hand-check ban (2004-05); the ABA is one period. |
| Scenario | A named, completed job that you can reopen and compare. |
| Season | An NBA season named by its end year. `SeasonYear(2004)` is 2003-04. |
| Seed | Playoff position 1 to 8 inside a conference. |
| Series | A best-of-five or best-of-seven playoff matchup. A `SeriesState` is either in progress or decided. |
| Store | The `PostgresStore` adapter, which implements every store port. |
| Team | A team code and era, such as SEA or OKC. Identified by `TeamId`. |
| Themed era | A curated span of NBA/BAA seasons named for how the game was played or followed (the Reddit NBA era, the three-point dominance era); a season belongs to every theme active in it. |
| Title HHI | The Herfindahl-Hirschman index of a group's titles: the sum of each champion's squared share. 1.0 when one franchise won every title; lower when the titles spread out. |
