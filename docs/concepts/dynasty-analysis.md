---
type: Method
title: Dynasty analysis
description: How the Dynasty Lab judges a franchise's title run against equal strength, a persistent alpha and historical Elo.
tags: [dynasty, concentration]
generated: { by: "human:abenamer", at: "2026-10-21T17:30:00Z" }
sources:
  - id: code
    resource: /src/services/dynasty.rs
    title: Dynasty service
---

# Season table

For a franchise and a season range, the page shows for each season: record, regular-season-end Elo, pre-playoff Elo, seed, actual result, and three title probabilities:

| Probability | Definition |
|-------------|------------|
| Historical Elo | Exact pre-playoff championship probability from the [playoff simulator](/concepts/playoff-engine.md). Zero when the team missed the playoffs. |
| Equal lottery | 1 / (number of teams in the league that season). |
| Persistent alpha | alpha / (alpha + N − 1) with the user's alpha for this franchise and 1 for every other franchise. |

# Significance

Seasons are independent trials with different probabilities. The number of titles follows a Poisson-binomial distribution. The page shows, for each model, the expected titles and the probability of at least the actual number of titles.

Example: Golden State 2014-15 to 2021-22, four titles. Historical Elo expects 1.93 titles and gives 2.8% to four or more. The equal lottery gives 0.01%.

# League-wide concentration

The page also feeds the per-season Elo probabilities of all 30 franchises into the [alpha lottery engine](/concepts/alpha-engine.md) in schedule mode. It compares the probability that some franchise wins K or more titles under equal strength, under the persistent alpha and under historical Elo.

# Caution

Franchise Elo measures team strength. It does not attribute a result to one player, a coach or an injury.
