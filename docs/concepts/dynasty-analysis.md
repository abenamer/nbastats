---
type: Method
title: Dynasty analysis
description: How the Dynasty Lab judges a franchise's title run against equal strength, a ticket lottery with alpha tickets for the franchise and historical Elo.
tags: [dynasty, concentration]
generated: { by: "human:abenamer", at: "2026-10-21T17:30:00Z" }
sources:
  - id: code
    resource: /src/application/dynasty.rs
    title: Dynasty service
---

# Season table

For a franchise and a season range, the page shows for each season: record, regular-season-end Elo, pre-playoff Elo, seed, actual result, and three title probabilities:

| Probability | Definition |
|-------------|------------|
| Historical Elo | Exact pre-playoff championship probability from the [playoff simulator](/concepts/playoff-engine.md). Zero when the team missed the playoffs. |
| Equal lottery | 1 / (number of teams in the league that season). |
| Ticket lottery (alpha) | alpha / (alpha + N − 1): the franchise holds alpha tickets in the season's title draw and every other franchise holds one. Alpha is a ticket count, not a rating. In a 30-team season alpha 29 is a coin flip for the title, and alpha 1 is the equal lottery. At the value that makes the expected titles equal the actual ones, the run is the expected outcome of that lottery; the [Alpha Simulator](/concepts/alpha-engine.md)'s backward view gives that value. The page says so. |

# Significance

Seasons are independent trials with different probabilities. The number of titles follows a Poisson-binomial distribution. The page shows, for each model, the expected titles and the probability of at least the actual number of titles.

Example: Golden State 2014-15 to 2021-22, four titles. Historical Elo expects 1.93 titles and gives 2.8% to four or more. The equal lottery gives 0.01%.

# Strength in Elo

The ticket count says nothing about how good the team was; this section does (`domain::simulation::dynasty::strength_seasons`). For each season of the window the field is every NBA team with a pre-playoff Elo. The franchise's standing in it is its rating against the league mean, against the best other team (the strongest team of the field other than the franchise) and against the worst team of the field, and its rank, with ties ranking the franchise first. A season with fewer than two rated teams has no field; a season the franchise was not rated in keeps the field's figures with the franchise's own as none. The summary over the seasons the franchise was rated in gives the mean gaps, the range of the gap above the mean, the mean rank and the seasons ranked first.

The page also reads the window's titles backwards, as the [Alpha Simulator](/concepts/alpha-engine.md)'s backward view does (`application::strength::read_record`): the Elo above the league mean a team of constant strength needed, against the real fields through a 16-team bracket from the seasons with at least sixteen rated NBA teams, for the titles to be its expected count, with the gap for even odds and the title chance of an average team. The reading needs at least one title and a window of at most forty seasons; one record is read at a time, and the page's reading waits its turn for up to thirty seconds. Otherwise the response says why there is none. A store failure is logged and reported as a reading not available, never as the failure's text.

# League-wide concentration

The page also feeds the per-season Elo probabilities of all 30 franchises into the [alpha lottery engine](/concepts/alpha-engine.md) in schedule mode. It compares the probability that some franchise wins K or more titles under equal strength, under the ticket lottery and under historical Elo.

# Caution

Franchise Elo measures team strength. It does not attribute a result to one player, a coach or an injury.
