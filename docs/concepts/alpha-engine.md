---
type: Model
title: Alpha lottery engine
description: Engine A simulates seasons as weighted lotteries and validates the result with an exact occupancy formula.
tags: [engine-a, alpha, monte-carlo, exact]
generated: { by: "human:abenamer", at: "2026-10-21T17:30:00Z" }
sources:
  - id: code
    resource: /src/simulation/alpha.rs
    title: Engine A source
  - id: exact
    resource: /src/simulation/exact.rs
    title: Exact occupancy calculation
---

# Model

Each season has one champion. Franchise i wins with probability:

    P_i = alpha_i / Σ_j alpha_j

Seasons are independent. When all alpha values equal 1, every franchise has the same probability.

# Inputs

| Input | Default |
|-------|---------|
| Franchises | 30 |
| Seasons | 22 |
| Iterations | 100,000 |
| Threshold K | 4 |
| Seed | user value |
| Alpha values | 1 per franchise, editable |
| Alpha mode | constant, or season-dependent (a full seasons × franchises table) |

# Outputs

* Probability that at least one franchise wins K or more titles, with a Monte Carlo standard error and a 95% interval.
* Probability that a chosen franchise wins K or more titles.
* Distribution of the maximum title count across franchises.
* Distribution of the number of franchises with K or more titles.
* Expected titles for each franchise.
* Top-five share, Gini coefficient and Herfindahl-Hirschman index of the title counts.
* Expected number of franchises with exactly j titles.

# Exact check

For constant alpha values, the engine also computes the exact answer:

    P(all franchises below K) = N! · [x^N] ∏_i Σ_{j<K} (p_i x)^j / j!

For 30 equal franchises, 22 seasons and K = 4, the exact probability that at least one franchise wins four or more titles is 16.05%. The unit test `converges_to_exact_occupancy_probability` checks that the Monte Carlo result agrees within four standard errors.[^exact]

# Determinism

The run is split into chunks. Chunk i uses the ChaCha8 stream i of the run seed. Chunk results are added. The result does not depend on thread scheduling.

# Scenarios

A named run is a scenario. The application stores its parameters, seed, result, timestamp, application version and model version. You can reopen and compare scenarios on the Alpha Simulator page. See [simulation jobs](/reference/jobs.md).
