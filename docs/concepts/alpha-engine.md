---
type: Model
title: Alpha lottery engine
description: Engine A, the alpha lottery. What question it answers, what the alpha values and the threshold mean, how to read its results, how a title record is read backwards into a strength, and the exact formula that checks it.
tags: [engine-a, alpha, monte-carlo, exact]
generated: { by: "human:abenamer", at: "2026-10-21T17:30:00Z" }
sources:
  - id: code
    resource: /src/domain/simulation/alpha.rs
    title: Engine A source
  - id: exact
    resource: /src/domain/simulation/exact.rs
    title: Exact occupancy calculation
  - id: strength
    resource: /src/domain/strength.rs
    title: Strength in Elo terms
---

# What it is for

The alpha lottery answers one question: **how often would a title run happen by chance, given how strong each franchise is?** It knows nothing about games, ratings or schedules. Each season is a draw from an urn: every franchise holds a number of tickets (its alpha), one ticket is drawn, and the holder is the champion. Run that for 22 seasons, repeat a hundred thousand times, and count how often some franchise ends up with four or more titles.

Use it when the question is about luck and concentration rather than about a particular season:

* Is four titles in eight years remarkable in a 30-team league?
* If one franchise is three times as strong as the rest, how often does it win four of ten?
* How many franchises should expect to win at least twice in a generation?

For a question about a real bracket or season, use the [playoff simulator](playoff-engine.md) or the [season forecast engine](season-engine.md). To judge a real run against the historical ratings, use the [Dynasty Lab](dynasty-analysis.md), which feeds this engine with Elo-derived probabilities. The step-by-step use of the page is in the runbook [Run an alpha scenario](/runbooks/alpha-scenario.md).

# Model

Each season has one champion. Franchise i wins with probability:

    P_i = alpha_i / Σ_j alpha_j

Seasons are independent. When all alpha values equal 1, every franchise has the same probability.

An alpha is a relative strength, not a probability: a franchise with alpha 4 among 29 others at 1 holds 4 of 33 tickets, a title probability of 12.1 percent each season, four times that of each other franchise. Scaling every alpha by the same factor changes nothing. The **threshold K** is the title count that defines a dynasty for the question asked: "K or more titles in the seasons simulated".

# Inputs

| Input | Default | Meaning |
|-------|---------|---------|
| Franchises | 30 | The number of franchises in the lottery, or the active NBA franchises by name. |
| Seasons | 22 | The number of seasons each simulated history runs. |
| Iterations | 100,000 | Simulated histories. More iterations narrow the Monte Carlo interval; the limit is `MAX_ITERATIONS`. |
| Threshold K | 4 | The title count that counts as a dynasty. |
| Seed | user value | The random seed; the same inputs and seed give the same result. |
| Alpha values | 1 per franchise, editable | Relative strengths. |
| Franchise of interest | the first franchise | The franchise whose own probability of K or more titles is reported. |
| Alpha mode | constant, or season-dependent | Constant alphas hold for every season. In the page's decay mode an alpha moves half of the way to 1 every half-life, so a strong start fades into the pack and an alpha below 1 rises toward 1 the same way; the API takes any full seasons × franchises table as `schedule`. |

# Outputs

* Probability that at least one franchise wins K or more titles, with a Monte Carlo standard error and a 95% interval.
* Probability that the franchise of interest wins K or more titles.
* Distribution of the maximum title count across franchises.
* Distribution of the number of franchises with K or more titles.
* Expected titles for each franchise.
* Top-five share, Gini coefficient and Herfindahl-Hirschman index of the title counts.
* Expected number of franchises with exactly j titles.

# Reading the results

* **P(any franchise ≥ K)** is the headline: the share of simulated histories in which some franchise, whichever, reached K titles. With 30 equal franchises over 22 seasons it is 16 percent for K = 4: a four-title dynasty happens by luck alone about one generation in six.
* **P(franchise of interest ≥ K)** is the same count for one named franchise. For an equal franchise it is far lower (0.6 percent for K = 4 over 22 seasons), because "any franchise" sums the chances of all thirty. With alpha 4 it rises to 27 percent.
* **E[# franchises ≥ K]** is the expected number of dynasties per history; the chart below the cards shows how that number is distributed.
* **Maximum titles by any franchise** shows how high the best run usually goes; the grey bars are the exact distribution when the alphas are constant.
* **Expected titles** per franchise is alpha share times seasons with constant alphas, and the sum of the per-season shares in season-dependent mode, where the Alpha column shows the starting alpha. The lighter bar is the simulated mean, which should match it within the Monte Carlo noise.
* **Top-5 share, Gini and HHI** measure how concentrated the titles were on average: the share of titles won by the five most successful franchises, the Gini coefficient of the title counts (0 when every franchise won the same number, towards 1 when one won all) and the Herfindahl-Hirschman index (the sum of squared title shares, 1 when one franchise won them all). Compare them with the historical values on the Dashboard.
* The **± value** on the headline is the 95 percent Monte Carlo interval. When the exact value is shown, it lies inside the ± range in 19 runs of 20; a gap of several times the ± range points to a defect. The unit test `converges_to_exact_occupancy_probability` checks four standard errors, and `same_seed_same_result` checks that a seed reproduces its run.

# Exact check

For constant alpha values, the engine also computes the exact answer:

    P(all franchises below K) = N! · [x^N] ∏_i Σ_{j<K} (p_i x)^j / j!

For 30 equal franchises, 22 seasons and K = 4, the exact probability that at least one franchise wins four or more titles is 16.05%. The unit test `converges_to_exact_occupancy_probability` checks that the Monte Carlo result agrees within four standard errors.[^exact]

In season-dependent mode there is no exact value, and the page says so; the Monte Carlo interval is the only error bar.

# Working backwards from the titles

The page also answers the question in reverse: how strong did a franchise have to be to win the titles it won? Pick the franchise, a window of seasons and the titles won; the record is read two ways, both computed exactly.

**As an alpha.** For `titles` titles in `seasons` seasons among `franchises` franchises, the implied title chance per season is titles divided by seasons, the chance that makes the titles the expected count and the maximum-likelihood chance for that record. Against every other franchise at alpha 1 it is the alpha

    alpha = (franchises − 1) · p / (1 − p)

Four titles in 22 seasons among 30 franchises give p = 0.182 and alpha 6.44: 6.4 times the tickets of each other franchise, 18 percent of the whole urn. The same four titles in the eight seasons of the run itself give p = 0.5 and alpha 29. An alpha is a ratio of title odds, not a measure of how much better the team was. The panel adds the 95 percent Wilson interval of the chance and the alphas at its ends (for 4 of 22, alpha 2.3 to 18.2, because a few titles are a small sample), the chance of at least that many titles at the implied strength (59 percent for 4 of 22: the record is the expected outcome and still fails four times in ten), the alpha at which that many titles becomes a coin flip (5.7), and the chance of the record for an ordinary franchise, luck alone (0.6 percent). A title every season has no finite alpha. `domain::simulation::exact::strength_for_titles` computes it.

**As a strength.** The alpha says how lopsided the odds had to be; the strength says how good the team had to be. For each season of the window the field is the pre-playoff Elo of every other team (the franchise's own rating left out). A team of constant strength is placed at the league mean plus a gap, the top sixteen by rating form a modern 16-team bracket seeded by rating with the two strongest in different halves, and the bracket's exact probabilities (the [playoff simulator](playoff-engine.md)'s model with the default home advantage) give the team's title chance; the chance averaged over the window's seasons rises with the gap, so bisection finds the gap at which it equals the record's chance. The gap is reported as Elo points above the league mean, as points per game (100 Elo is 3.5 points) and as the record such a team would post over 82 games against average opponents, half at home. The same gaps are given for the ends of the record's range and for even odds, with the title chance of an average team against those fields, and with what the franchise actually rated before the playoffs, averaged over the window with missed postseasons included, so the two can be compared. `domain::strength` computes it.

The field matters more than the formula. Against 29 equal teams a 50 percent title chance needs only about 118 Elo above the mean, because the top seed with home court in every round wins four series at 84 percent each. Against the real fields, where the best team enters the playoffs about 170 to 285 Elo above the mean in the modern seasons, the same chance needs a gap in the hundreds. The chance rises with the gap apart from small steps where the team overtakes another and changes seed, so each reading also carries the chance its gap really has, and the page shows it when it differs. A title every season has no finite gap; a window in which no season has at least sixteen rated teams gives no strength reading; the alpha reading counts the franchises of the last season with a field. One reading runs at a time, on a thread of its own, because a long window costs seconds of exact brackets (`application::strength` builds the fields and runs it).

"Use this alpha" puts the implied alpha on the franchise of interest, with the window's length as seasons and the titles as the threshold, so the forward simulation can show the whole distribution at that strength.

# Limits

* Seasons are independent draws: no carry-over of strength, no ageing, no draft, no cap, unless you encode them in the alpha table.
* Nothing in the engine is calibrated to the NBA. The alphas are your assumption; the engine only turns them into probabilities consistently.
* The result is a probability under the model, not a verdict on a real team. The Dynasty Lab is the place where real titles meet these probabilities.

# Determinism

The run is split into chunks. Chunk i uses the ChaCha8 stream i of the run seed. Chunk results are added. The result does not depend on thread scheduling.

# Scenarios

A named run is a scenario. The application stores its parameters, seed, result, timestamp, application version and model version. You can reopen and compare scenarios on the Alpha Simulator page. See [simulation jobs](/reference/jobs.md).
