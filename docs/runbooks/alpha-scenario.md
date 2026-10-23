---
type: Runbook
title: Run an alpha scenario
description: Use the Alpha Simulator to find how often a title run happens by chance under assumed franchise strengths, save the scenario and compare it with others.
tags: [engine-a, alpha, runbook, scenarios]
generated: { by: "human:abenamer", at: "2026-10-23T18:00:00Z" }
---

# Before you start

* The application runs and the Alpha Simulator opens at `/alpha` (see [Start the application](start.md)).
* Read [Alpha lottery engine](/concepts/alpha-engine.md) for what the alpha values and the threshold mean. In short: each season one franchise wins, with a probability proportional to its alpha; the threshold K is the number of titles that counts as a dynasty.

# Procedure A: how often does a dynasty happen by luck alone?

1. Leave the defaults: 30 franchises, 22 seasons, 100,000 iterations, threshold 4, every alpha 1. Twenty-two seasons is about one generation of fans; four titles is the usual bar for a dynasty.
2. Read the exact check under the results before you run: for 30 equal franchises over 22 seasons it says 16.05 percent.
3. Click **Run simulation**. The status shows the progress; a run of this size takes a few seconds.
4. Read the first card, **P(any franchise ≥ 4)**. It should be close to 16 percent and the exact value is printed beside it. The second card gives the same probability for one named franchise, which is far lower because the first card sums the chances of all thirty.
5. Change **Threshold K** to 3 or 5 and run again to see how the bar moves. Change **Seasons** to 10 for a question about a decade.

# Procedure B: one franchise stronger than the rest

1. Click **Use NBA franchise names** so the list shows the active franchises by key.
2. In the alpha editor, set the alpha of the franchise you ask about to 4 and leave the others at 1. That franchise now holds 4 tickets of 33: a title probability of 12.1 percent each season, four times that of each other franchise.
3. Choose the same franchise under **Franchise of interest**.
4. Type a name under **Name**, for example `GSW alpha 4`, so the run is saved as a scenario.
5. Click **Run simulation**. **P(franchise of interest ≥ 4)** should be about 27 percent; its expected titles about 2.7 over 22 seasons.
6. Try alpha 2, 3 and 6 as separate scenarios, and change the name for each run (for example `GSW alpha 2`): the Name field keeps its value between runs and names are not unique. The sum of alphas changes with each value, so the other franchises' probabilities move a little too.

# Procedure C: a strength that fades

1. Set **Alpha mode** to "Season-dependent: alpha decays toward 1" and a **Half-life** of 5 seasons. The alpha of every franchise then moves half of the way to 1 every five seasons: a franchise at 4 is at 2.5 after five seasons, at 1.75 after ten, and part of the pack after twenty. An alpha below 1 rises toward 1 the same way, so set a franchise to 1, not 0, to make it ordinary.
2. Run. There is no exact value in this mode; the ± value on the first card is the Monte Carlo interval.
3. Raise **Iterations** when the interval is too wide for the comparison you make. The limit is `MAX_ITERATIONS` (see the [configuration reference](/reference/configuration.md)).

# Compare scenarios

1. Under **Saved scenarios**, tick two or more runs and click **Compare selected**. The table lists the headline probabilities, each run's franchise of interest and the concentration measures side by side with the versions that produced them.
2. **Open** restores a scenario's inputs into the form, the alpha mode and the half-life included; **Delete** removes it.
3. A scenario keeps its seed, so running it again after Open gives the same result.

# Checks

* With constant alphas the exact value lies inside the printed ± range in 19 runs of 20; a gap of several times the range points to a defect.
* With constant alphas the expected titles of a franchise equal its alpha share times the number of seasons; in season-dependent mode they are the sum of its per-season shares.
* Every alpha at 1 gives every franchise the same probability; the Dashboard's equal-strength comparison uses the same engine.
