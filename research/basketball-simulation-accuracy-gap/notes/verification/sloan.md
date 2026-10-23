# Verification of the Sloan-track papers against their full text

Checked on 2026-10-23 against research/basketball-simulation-accuracy-gap/report.md ("the report") and notes/sloan_papers.md ("the notes"). The other four notes files and research/papers/README.md were searched for the authors, titles and years; only README.md mentions three of these papers (Oh, Sandholtz, Sill), and those lines are checked under each paper. Page numbers are PDF page numbers. Every page of every PDF was read (text layer), and the first page plus the figure pages cited below were rendered and inspected.

Summary of the verdicts:

- The report's headline claim that no Sloan paper publishes a game-winner accuracy against a betting line is wrong: the 2026 deep-reinforcement-learning paper reports 66.3 percent (67.1 with synergies) against 68.7 percent for the Vegas closing line on 2,235 held-out games (Table 8, page 23). No Sloan paper publishes a log loss or Brier score against a line.
- Oh, Keshri and Iyengar do publish an accuracy figure, but at the team-season level (out-of-sample R-squared 0.87 on win percentages), not at the game level.
- Gong and Chen's "improved prediction accuracy" claim does not exist in the PDF; the paper has no prediction test at all.
- Sandholtz and Bornn assume lineup invariance; the paper is not evidence that lineups matter.
- Sill's quote is slightly misworded; the paper never uses the term RAPM; its only baseline is the mean home margin, not Vegas.
- Rafique and Sanders is a 2026 paper (fixed by its own text); the quoted Play-In sentence is not in the PDF.
- Two PDFs are anonymised competition submissions (no authors, year or venue on the page): the positional plus-minus paper (ID 111280) and the deep-RL paper (Paper ID 137). Two PDFs carry publisher copyright lines: Sandholtz-Bornn (Institute of Mathematical Statistics) and Maymin-Maymin-Shen (West Virginia University).

---

## 1. Oh, Keshri, Iyengar 2015, "Graphical Model for Basketball Match Simulation"

Local copy of Oh, Keshri, Iyengar 2015, read on 2026-10-23 (8 pages; pdfinfo: pdfTeX, created 2014-12-14).

### (1) Bibliographic facts

- Authors: Min-hwan Oh, Suraj Keshri, Garud Iyengar, Columbia University (page 1).
- Year and venue: 2015; page header "9th Annual MIT Sloan Sports Analytics Conference, February 27-28, 2015, Boston Convention and Exhibition Center"; footer "2015 Research Paper Competition, Presented by: ticketmaster" with a "42 Analytics" logo (every page).
- Title: the rendered title on page 1 reads "Graphical Model for Basketball Match Simulation"; the PDF's extracted text layer reads "Graphical Model for Baskeball Match Simulation", so the Sloan site's "Baskeball" comes from the paper's own file.
- Licence or copyright line: none on any page. Only the conference header and sponsor footer.
- File name: correct as is.

### (2) Claims attributed to this paper

Report, section "Sloan rewards new data and decisions":

- "the only paper whose stated purpose is simulating NBA matches is Oh, Keshri and Iyengar's 2015 'Graphical Model for Basketball Match Simulation,' which models each touch as a transition between discrete states over a network of the players on court, evaluates on 2013-14 data" — confirmed (abstract, page 1: "We model every touch and event in a game as a sequence of transitions between discrete states ... each node is a network structure of players on the court"; page 2: "calibrated using the player tracking data and play-by-play game log data from the matches for the 2013-2014 season").
- "concludes qualitatively that the approach 'is appropriate for modeling a basketball match'" — confirmed (page 1: "Evaluation on the match data for the 2013-14 NBA season suggests that the graphical model approach is appropriate for modeling a basketball match"). "Qualitatively" is wrong in spirit: the conclusion rests on the R-squared figures below (page 5).
- "'changes in the team lineup or the opponent's lineup significantly affect the dynamics of a match'" — corrected wording. The paper says "either changes in the team lineup or changes in the opponent team lineup significantly affects the dynamics of a match progression" (page 1) and "either changes in the team lineup or in the opponent team lineup significantly affects the dynamics of a match progression" (page 7).
- "no accuracy, log loss or market comparison surfaced in any snippet of the paper, its Semantic Scholar record or Keshri's Columbia dissertation, which extends the evaluation to 2013-16" — corrected. The paper does report accuracy figures, at the team-season level: "We used 70% of 1230 matches in the regular season as the training set and the remaining 30% as the test set. Figure 3 shows that the model provides a good estimate of the teams' actual win percentages with the within-sample R-squared 0.92, and the out-of-sample R-squared 0.87" (page 5, Figure 3); player points per game R-squared 0.74 (training) and 0.63 (test) (page 6, Figure 4); "Our model's performance in predicting average winning percentage is comparable to Shirley [1] and Štrumbelj and Vračar [2]" (pages 5-6). There is no game-level accuracy, no log loss, no Brier score and no market comparison (confirmed absent). The dissertation is not in the repository and cannot be checked.
- Conclusion: "the Oh-Keshri-Iyengar PDF, for the one Sloan simulation accuracy that may exist" — resolved: the accuracy that exists is the season win-percentage R-squared above.
- "The only playoff simulation at Sloan is Rafique and Sanders' cross-sport tournament-design study" — corrected in part: Oh et al. "simulated the 2014 NBA Finals with two distinct sets of lineups of the Spurs while keeping the lineups of the Heat fixed ... 101 simulations were performed to determine a single win ... We applied conventional best-of-seven playoff format to determine the winner of the Finals" (page 7, Figure 6: outcomes 4:2 and 3:4). It is a demonstration of lineup sensitivity, not a forecast.

Notes (sloan_papers.md):

- Inventory entry: "Probabilistic graphical model of match progression; each node is a network structure of the players on court, their actions and events; each touch is a transition between discrete states" — confirmed (page 1-2).
- "evaluated on 2013-14 NBA data" — confirmed (pages 2, 5), with the 70/30 split above.
- "(abstract snippet; the site misspells 'Baskeball')" — the PDF's text layer has the same misspelling (page 1).
- "This is the only Sloan paper I found whose stated purpose is simulating NBA matches." — consistent with the PDF ("Most of the previous simulation approaches in basketball have focused on win-loss predictions, ignoring the progression of matches", page 1).
- KQ2: "conclusion is qualitative ... no accuracy, RMSE, log loss or market comparison appeared in any snippet" — corrected as above (R-squared 0.92 / 0.87, page 5).
- KQ3: "Lineups: 'changes in the team lineup or the opponent's lineup significantly affect the dynamics of a match' (2013-14 data)" — confirmed in substance; exact wording on pages 1 and 7 as quoted above.
- KQ4: "match-level simulation evaluated on 2013-14 data; no market baseline mentioned" — confirmed that there is no market baseline; the evaluation is at season win-percentage level, not match level.
- KQ2 Gaps: "the Oh-Keshri-Iyengar PDF and Keshri dissertation are the top priority to read for a possession-simulation accuracy number" — resolved: no possession- or game-level accuracy number exists in the paper.

README.md line 9: "Oh, Keshri, Iyengar 2015, Graphical Model for Basketball Match Simulation. MIT Sloan; the PDF is on the conference site (open)." — consistent with the PDF (no licence line; conference competition paper).

### (3) Figures in the paper the report does not yet use

- Data: NBA.com player tracking and play-by-play for 2013-14, lineup data from Basketball-Reference (page 2).
- Game outcome in simulation is a majority vote over repeated simulations: "We then assign a win to a team with more number of wins" (page 5); 101 simulations per game in the Finals demonstration (page 7). The model therefore never outputs a calibrated win probability, which is why no Brier or log loss could be reported.
- The team-season R-squared of 0.87 is computed on the 30 teams' win percentages over the held-out 30 percent of the same season's games; it is a within-season fit, not a forward forecast, and a record-based predictor would score comparably (my inference, not the paper's).
- The paper cannot estimate pass networks for hypothetical lineups mixing players from different teams, and does not model assists or blocks (page 7).
- Motivation names injuries and day-to-day lineup changes as the reason team-level simulators fail: "changes in lineups of teams during a season, which appear almost on a day-to-day basis ... whether it is due to injuries or strategic reasons. These changes do have an impact on final game results" (page 1).

### (4) Relevance to an Elo-based Monte Carlo championship simulator

A player-identity possession simulator whose only published accuracy is a season win-percentage R-squared of 0.87 on 2013-14, with no game-level, log-loss or market comparison; it supports the report's point that richer simulation at Sloan was never benchmarked as a forecaster, and its lineup-sensitivity demonstration supports roster awareness as the omitted variable.

---

## 2. Sandholtz and Bornn 2020, "Markov Decision Processes with Dynamic Transition Probabilities: An Analysis of Shooting Strategies in Basketball"

Local copy of Sandholtz, Bornn 2020, read on 2026-10-23 (24 pages).

### (1) Bibliographic facts

- Authors: Nathan Sandholtz and Luke Bornn, Department of Statistics and Actuarial Science, Simon Fraser University (page 1).
- Venue and year: The Annals of Applied Statistics, 2020, Vol. 14, No. 3, 1122-1145, https://doi.org/10.1214/20-AOAS1348; "Received December 2018; revised March 2020" (page 1).
- Copyright line, page 1: "© Institute of Mathematical Statistics, 2020". This is the published journal version, not the arXiv or Sloan version. README.md says the AoAS version is "paywalled: link only" and that "the authors' site offers a PDF"; the file in the repository is the journal typesetting with the IMS copyright line, so its presence should be reconciled with the README rule.
- The title "Replaying the NBA" used in the report and notes does not appear anywhere in this PDF; the 2018 Sloan proceedings version is not in the repository and its title, year and page range cannot be verified from this file.
- File name: correct for the version held (sandholtz-2020-...).

### (2) Claims attributed to this paper

Report:

- "'Replaying the NBA' (Proceedings of the 12th SSAC, 2018, later Annals of Applied Statistics 2020)" — the AoAS 2020 part is confirmed (page 1); the Sloan part is not in the paper.
- "models every possession as a non-stationary Markov decision process with shot-clock-dependent transitions" — corrected in detail. The unit is a "play", an episode that terminates in a shot or turnover: "we model basketball plays as episodes from team-specific nonstationary Markov decision processes (MDPs) with shot clock dependent transition probabilities" (page 1); "we have omitted plays that ended in fouls and any free throw situations" (page 11, footnote 6); "we do not analyze rebounding; the number of plays is fixed beforehand, and once a turnover happens or a shot is taken, the play ends" (page 14).
- "combines lineup-specific MDPs into team-average MDPs" — confirmed as the abstract's wording (page 1), but the body states the opposite assumption: "Because most teams use upward of 500 lineups over the course of a season, we assume that P(·), R(·) and π(·) are invariant to the lineup, that is, other players do not impact transitions and rewards" (page 5). The weighting scheme averages chains with overlapping state spaces; it does not estimate lineup effects.
- "simulates whole seasons at sub-second resolution" — confirmed ("simulate from these processes not simply by outcome but rather at the subsecond level", page 2; 300 simulated seasons for the Cavaliers, pages 15 and 18).
- "its baseline is the on-policy simulation of the same team under altered shot policies, not a market or an Elo forecast" — confirmed (pages 15-20).
- "two match or season simulators (Oh, Keshri and Iyengar 2015; Sandholtz and Bornn 2018)" — the simulator simulates plays and aggregates to season totals of shots and points; it never simulates game winners, and the defense "is not an adversarial agent in our model" (page 3).

Notes:

- "Proceedings of the 12th MIT SSAC, 2018, pp. 1-13" — not verifiable from this PDF.
- "Models each possession as a non-stationary Markov decision process with shot-clock-dependent transitions estimated by Bayesian hierarchical models" — confirmed except "possession" (see above).
- "lineup-specific MDPs are combined into team-average MDPs" — see the lineup-invariance correction above (page 5).
- "the simulator runs at sub-second level and records every on-ball event" — confirmed (page 2).
- "seasons are simulated on-policy and under altered shot policies to measure net changes in efficiency and production (case: 2015-16 Cleveland Cavaliers)" — confirmed (pages 18-20; data are STATS LLC optical tracking for 2015-16, page 3).
- "Annals of Applied Statistics 14(3), 1122-1145 (2020)" — confirmed (page 1).
- KQ2: "outputs are distributional estimates of counterfactual season production under altered shot policies, propagated through posterior samples; the search did not surface a forecast-accuracy metric" — corrected: validation is by out-of-sample log-likelihood on "a sample of approximately 28,000 plays" held out of 155,656 plays (Table 2, page 13) and by calibration of 300 simulated seasons of transition counts against the observed counts (Figure 6, pages 15-17). There is no win-loss or margin forecast metric.
- KQ3: "Lineups: lineup-specific MDPs combined into team-average MDPs via a transition weighting scheme; Bayesian hierarchical models borrow strength across players and time; uncertainty propagated via posterior samples" — the hierarchical-model and posterior-sample parts are confirmed (pages 8-14); the lineup part should say the model assumes lineup invariance, so it is not evidence for the KQ3 takeaway that "lineups matter".
- KQ4: "after calibration, seasons simulated on-policy and under altered shot policies; baseline is the on-policy simulation, not a market" — confirmed.

README.md line 11 — see the copyright note under (1).

### (3) Figures in the paper the report does not yet use

- "most teams use upward of 500 lineups over the course of a season" (page 5), a published counterpart to the "more than 600 lineups" figure the report takes from arXiv 2601.15000.
- Policy effects are small in points per play: cutting contested midrange shots by 70 percent and doubling three-point policies raises expected points per shot from 1.04 to 1.09 and per play from 0.92 to 0.97 (page 19); a youth-development passing policy costs 0.02 points per play (page 20).
- Suggested use for injuries: "our methods could enable teams to gauge the effects of having to play second-string players if any starters suffered a long-term injury" (page 20), i.e. the lineup question is deferred to future work.
- Observational evidence that defenses do not fully neutralise a policy change within a season (Toronto 2016-17 to 2017-18, page 18).

### (4) Relevance

A counterfactual play simulator for shot-policy analysis that assumes lineup invariance and never forecasts game outcomes; it offers no benchmark for or against a team-Elo Monte Carlo and should not be cited as evidence that lineups matter.

---

## 3. Sill 2010, "Improved NBA Adjusted +/- Using Regularization and Out-of-Sample Testing"

Local copy of Sill 2010, read on 2026-10-23 (7 pages).

### (1) Bibliographic facts

- Author: Joseph Sill, joe@hoopnumbers.com (page 1).
- Year and venue: "presented at the MIT Sloan Sports Analytics Conference on March 6, 2010" (page 1 header); MIT SSAC and 42 Analytics logos.
- Licence or copyright line: none.
- File name: correct as is.

### (2) Claims attributed to this paper

Report:

- "won the best non-academic paper prize" — not in the paper.
- "is the origin of RAPM" — the paper never uses the term "RAPM"; it describes "a Bayesian technique called regularization (a.k.a. ridge regression)" (page 1) and refers to "regularized adjusted plus/minus results" on the author's site (page 6).
- "reports a test RMSE of 11.47 points (R-squared 0.169) on game margins for one configuration" — confirmed (page 6: "For three years of data, the optimal combination of meta-parameters was λ=3000, M=1200, and D=0.25. This model achieves a test RMSE of 11.47 (R-squared 0.169)"). Test set: the March and April games of 2008-09; training data '06-'07 to '08-'09 through February (page 4).
- "finds that 'the benefit of additional years of data and the use of a minutes cutoff turns out to be modest once regularization is applied'" — corrected wording. The sentence is "When using regularization, the benefit of additional years of data and the use of a minutes cutoff turns out to be modest" (page 6). The one-year, no-cutoff model (λ=2000) scores RMSE 11.54 (R-squared 0.159), "nearly as good as the best result achieved with 3 years of data" (page 6).
- "More history will not: Sill found extra years modest once regularized" — confirmed with a nuance: without regularization extra seasons were "highly beneficial" (one-year test RMSE 12.76, worse than the trivial model; three-year 12.01, R-squared 0.088; page 5).
- "a lineage of player-rating papers validated by out-of-sample game prediction (Sill 2010 onward)" — confirmed for Sill: "APM models are evaluated in terms of their ability to predict the outcome of future games not included in the model's training data" (page 1); the predicted quantity is the game's margin of victory, summed from per-snippet efficiency predictions, scored by RMSE (page 4).

Notes:

- "Presented at SSAC on March 6, 2010" — confirmed (page 1).
- "introduces ridge regression ('RAPM'), described as nearly doubling APM's accuracy" — "nearly doubles its accuracy" is confirmed (abstract, page 1; R-squared 0.088 to 0.169); "RAPM" is not the paper's term.
- KQ2: "test RMSE 11.47 (R-squared 0.169) for one model configuration" — confirmed. "No Vegas comparison surfaced in any snippet" and Gaps "Whether Sill (2010) compared against Vegas spreads is unknown" — resolved: there is no Vegas comparison. The only baseline is "predicting the mean home margin of victory of 3.61 points", RMSE 12.58 (page 5).
- KQ3: "once ridge regularization is applied, extra years of data and minutes cutoffs give only modest gains" — confirmed (page 6).
- Inference "Sill's 11.47-point RMSE on margin is a usable reference point for a point-spread-error metric" — note that the 2026 deep-RL paper gives a directly comparable published pair: Vegas closing-spread RMSE 10.42 against RAPM-based 11.24 on 2,235 games (see paper 7).

README.md line 12: consistent (conference paper, no licence line).

### (3) Figures in the paper the report does not yet use

- Standard deviation of the margin of victory in the test games is 12.58, so the best model explains 17 percent of margin variance out of sample (page 5).
- Mean home margin in the March-April 2008-09 test games: 3.61 points (page 5).
- The optimal ridge prior has standard deviation about 3 points per 100 possessions, so "95% of the players a priori are presumed to be within the range [−6, 6]", and "1 point per game in margin of victory over the course of an 82 game season is worth nearly 3 wins on average" (page 6).
- Unregularized one-season APM tuned by cross-validation scored worse out of sample (12.76) than the trivial home-court model (12.58) (page 5).

### (4) Relevance

Shows that even a well-regularized player-level rating explains only about 17 percent of out-of-sample margin variance (RMSE 11.47 against 12.58), which bounds what bottom-up ratings can add to a team-level Elo; its 3.61-point mean home margin for 2008-09 is a usable home-court reference.

---

## 4. Goldman and Rao 2013, "Live by the Three, Die by the Three? The Price of Risk in the NBA"

Local copy of Goldman, Rao 2013, read on 2026-10-23 (15 pages).

### (1) Bibliographic facts

- Authors: Matthew Goldman (UC San Diego, Dept. of Economics) and Justin M. Rao (Microsoft Research) (page 1).
- Year and venue: page 1 header "7th Annual MIT Sloan Sports Analytics Conference, March 1-2, 2013"; footer "2013 Research Paper Competion [sic], Presented by: SAP" with a 42 Analytics logo (every page).
- Licence or copyright line: none.
- File name: correct as is.

### (2) Claims attributed to this paper

Report: "strategy (foul trouble in 2011, three-point risk in 2013)" — confirmed (2013, page 1).

Notes:

- "Game-theoretic model of shot selection under win-maximization; four years of play-by-play" — confirmed (abstract, page 1: "Using four years of play-by-play data"; Table 1: 120 team-years, 481,544 shots, page 6). The introduction says "six years of NBA games" (page 2), an internal inconsistency in the paper; the regressions use four seasons.
- "testable condition that 3-point success must fall relative to 2-point success as risk appetite rises; held only for trailing teams, leading teams became risk-averse 'contrary to what their risk preferences ought to be to maximize the chance of winning the game'" — confirmed (page 1 abstract; Proposition 4 page 5; Table 2 page 8; conclusion page 9).
- "Win probability is the objective function, not a published forecast." — confirmed: the win-probability model is a probit on score margin, time remaining, team quality and home court (Appendix 6.4, page 15; Appendix Figure 1, page 10), used only to compute the win value of 2s and 3s.
- KQ3: "Risk behaviour: leading teams tighten up contrary to win-maximization, trailing teams take optimal risk and show an efficiency boost" — confirmed (pages 6-9).

### (3) Figures in the paper the report does not yet use

- The "rubber band effect": trailing teams get more efficient on both shot types and leading teams tighten up, so "More games tend to be close late than we'd otherwise expect to observe" (page 8). Margins of victory are therefore compressed relative to a model of independent possessions; this bears on margin-of-victory multipliers in Elo updates (my inference).
- Clutch performance scales with quality: the slope of clutch bonus on baseline offensive efficiency is 1.65 (t = 7.27) and on defensive efficiency 1.09 (t = 4.20), so "the better team will have an advantage in the clutch on both sides of the ball and will thus tend to pull out more close games than their baseline advantage would suggest" (page 9); "good teams have an even greater advantage when the chips are down" (pages 3, 10).
- The win-probability model is a normal CDF in the lead plus remaining possessions times the per-possession mean difference, scaled by the root of remaining possessions (equation 8, page 15), the same structure as Stern's model used by Maymin et al.

### (4) Relevance

No forecast benchmark; its rubber-band and clutch findings imply that game margins understate the gap between good and bad teams, which argues for a chalk-favouring interpretation of close games rather than against it.

---

## 5. Maymin, Maymin and Shen 2012 (Sloan 2011), "How Much Trouble is Early Foul Trouble? Strategically Idling Resources in the NBA"

Local copy of Maymin, Maymin, Shen 2012, read on 2026-10-23 (16 pages).

### (1) Bibliographic facts

- Authors: Allan Maymin (AllianceBernstein), Philip Maymin (NYU-Polytechnic Institute), Eugene Shen (AllianceBernstein) (page 1).
- Year and venue: International Journal of Sport Finance, 2012, 7, 324-339 (page 1). This is the journal version, not the 2011 Sloan paper; the acknowledgements thank "participants at the 5th Annual MIT Sloan Sports Conference" (page 15), consistent with a 2011 Sloan presentation.
- Copyright line, page 1: "International Journal of Sport Finance, 2012, 7, 324-339, © 2012 West Virginia University". No open licence is shown; under the README rule ("a paywalled PDF is never committed") this file needs a licence check.
- File name: correct for the journal version.

### (2) Claims attributed to this paper

Report: "foul trouble in 2011" — consistent with the 5th SSAC (2011) acknowledgement (page 15).

Notes:

- "Uses NBA play-by-play data and a win-probability method (borrowed from financial economics / option value) to derive the 'Q+1' rule for benching a starter in foul trouble" — corrected in part. Data: 2006-07 to 2009-10, 1,149 + 1,183 + 1,176 + 1,215 = 4,723 games with usable data (page 5). The "Q+1" rule is not derived; it is the rule coaches already use ("Many coaches seem to bench on a 'Q+1' basis", page 3; coaches benched 72 percent of players after a Q+1 threshold foul, page 5), and the paper tests it, finding "yanking on a 'Q+1' basis is generally a good strategy" (page 3) and that Q+1 is the best-fitting definition of foul trouble (Table 4, page 13). The method extends Stern's (1994) Brownian-motion win-probability model with a jump-diffusion variance term and option-value reasoning (pages 6-7).
- "the authors say the win-probability method can be applied to other problems" — confirmed ("a novel win-probability approach that can be easily extended to other applications", page 1; page 14).
- "journal version International Journal of Sport Finance 7(4), 324-339 (2012)" — confirmed (page 1: Volume 7, Number 4, 2012).

### (3) Figures in the paper the report does not yet use

- Home-court advantage at tip-off: "For a game between two evenly matched teams with no possession advantage, the home team has a 62% chance of winning at the start of the game" (page 12, Figure 4), estimated on 2006-07 to 2009-10 play-by-play. This matches Lopez, Matthews and Baumer's 62.0 percent that the report already cites and is further published evidence that a 100-Elo home term (64 percent) is too strong.
- A foul-troubled starter left on the floor cuts the home team's tip-off win probability from 62 to 58 percent; two cut it to 53 percent (page 12, Figure 5). The win-probability coefficient on net Wins Produced per 48 minutes is 1.572, and foul trouble makes a star "perform worse than an average player" (page 11).
- A possession is worth about half a point (possession coefficient roughly half the lead coefficient, page 12).
- Score variance declines linearly with time at rate γ = 0.9958 < 1, so scores "do not diffuse normally and do appear to have jumps" (page 10-11, Table 3).
- The tied-game home win probability "dips slightly below 50% in the last minute of play" (page 12).
- Standard errors are deflated by a factor 15.8 for overlapping observations (Appendix A, page 16); the key coefficient is significant only at the 15 percent level (page 11).

### (4) Relevance

No forecast benchmark; its 62 percent home win probability for evenly matched teams over 2006-10 supports the report's recommendation to lower the fixed 100-Elo home term, and its in-game model is a published template for the normal-CDF win-probability form.

---

## 6. [Anonymous competition PDF] "Estimating Positional Plus-Minus in the NBA" (Gong and Chen 2023 per the Sloan listing)

Local copy of Gong, Chen 2023, read on 2026-10-23 (first filed as anonymous, attributed that day) (19 pages; pdfinfo: Microsoft Word 2019, created 2023-03-02).

### (1) Bibliographic facts

- The PDF is an anonymised research-paper-competition submission. Page 1 shows only the title, the track "Basketball" and the submission number "111280". No authors, year, venue or abstract appear anywhere in the file.
- Licence or copyright line: none.
- Attribution to Hua Gong and Su Chen, 2023, rests on the Sloan page and the JQAS record cited in the notes, not on the PDF.
- Proposed citation: Gong, Chen 2023 (author and year from the Sloan listing; the file itself is anonymous, which the README or a note should say).

### (2) Claims attributed to this paper

Report:

- "Gong and Chen's 2023 positional plus-minus claims 'improved prediction accuracy in player plus-minus when factoring in player positions' over 2015-16 to 2021-22 without a magnitude in the abstract" — not in the paper. The PDF contains no abstract, no prediction-accuracy claim, no out-of-sample test and no accuracy metric. The quoted phrase does not occur. The seasons are 2015-16 to 2021-22 with 2019-20 excluded ("We exclude the NBA 2019-20 data as the season was interrupted by the COVID-19 pandemic", page 4), six seasons in total.
- "a lineage of player-rating papers validated by out-of-sample game prediction (Sill 2010 onward)" — this paper is not validated by game prediction and should not be counted in that lineage.

Notes:

- "Hierarchical Bayesian linear model of plus-minus at the position level, 2015-16 through 2021-22" — confirmed (pages 1, 5-6; Gibbs sampler with 30,000 iterations and 2,000 burn-in, page 16), with 2019-20 excluded.
- "reports 'improved prediction accuracy in player plus-minus when factoring in player positions'" — not in the paper.
- KQ2: "magnitude and metric not in the abstract snippet" — there is no metric anywhere in the paper.
- KQ3: "including player positions improves prediction of player plus-minus" — not in the paper; "versatile offensive players and paint-defending bigs are the most valuable contributors" — confirmed ("Versatile Scorers are the most valuable offensive contributors in most seasons", pages 8-9; Bigs have the highest defensive plus-minus in every season, page 10).

### (3) Figures in the paper the report does not yet use

- Seven offensive positions from K-means++ on Synergy play-type shares and three defensive positions from official positions (pages 4-7); about 20 percent of players per season are "low usage" with no play-type data (page 3); 52,784 to 62,133 offense-defense stints per season (page 4).
- Positional offensive plus-minus gaps have narrowed over the six seasons, driven by Roll and Cut Bigs (pages 9, 13); Guards have the highest variance of defensive plus-minus (page 11).
- The authors' own limitation points at win probability: points per 100 possessions "does not reflect the importance of points in a game ... Future studies of positional plus-minus may consider using win probability as the response variable" (page 15).
- Garbage time is 3.4 to 4 percent of possessions and removing it does not change the estimates except in 2020-21 (page 14).

### (4) Relevance

None for forecasting: a descriptive study of positional plus-minus levels with no predictive test; the report's attribution of a prediction-accuracy claim to it should be withdrawn.

---

## 7. [Anonymous competition PDF] "Deep Reinforcement Learning for NBA Player Valuation: A Temporal Difference Approach with Shapley Attribution" (Jenkins 2026 per the Sloan event page)

Local copy of Jenkins 2026, read on 2026-10-23 (first filed as anonymous 2025, attributed that day) (28 pages; pdfinfo title "Final NBA DRL Sloan Paper.docx", Google Docs renderer, no creation date).

### (1) Bibliographic facts

- The PDF is an anonymised competition submission: page 1 shows the title, "Basketball Track" and "Paper ID 137". No authors, affiliation, year or venue appear in the file.
- Internal dating: data are "NBA play-by-play data from the 2020-21 through 2023-24 seasons ... 4,770 regular-season games" (page 10); the newest reference is 2022; the file title says "Sloan Paper". Consistent with the 20th SSAC (March 2026) identified in the notes; the attribution to Ben Jenkins, Florida Atlantic University, rests on the Sloan event page, not the PDF.
- Licence or copyright line: none.
- Proposed citation: Jenkins 2026 (conference year, matching the Rafique, Sanders 2026 convention; author from the event page; the file is anonymous). The earlier "2025" conflicted with the report and notes, which call it the 2026 paper.

### (2) Claims attributed to this paper

Report:

- "across 2007-2026 the Sloan basketball track produced ... a lineage of player-rating papers validated by out-of-sample game prediction (Sill 2010 onward), but none publishes a game-winner accuracy, log loss or Brier score against a betting line" — corrected. Table 8, "Game Outcome Prediction Accuracy" (page 23): Vegas Line (Benchmark) 68.7 percent, margin RMSE 10.42; BPM-based 62.4, 11.87; RAPM-based 64.8, 11.24; DRL-Shapley 66.3, 10.89; DRL-Shapley + Synergy 67.1, 10.71. "Win Prediction is accurate on 2,235 test games. Margin RMSE in points. Vegas Line uses closing spreads" (page 24). Validation is forward-chaining with three folds, one held-out season each (page 11). No log loss or Brier against the line is given (the Brier scores in Table 2, page 11, are for in-game win probability against logistic, random-forest and neural baselines: 0.168 against 0.198, 0.187 and 0.179).
- "the 2026 deep-reinforcement-learning valuation paper by Jenkins trains on a distributional win-probability model and claims 'better predictive accuracy than RAPM,' again without numbers in the abstract" — corrected: numbers as above; the paper states "It significantly outperforms both BPM (66.3% vs. 62.4%, p < 0.001) and RAPM (66.3% vs. 64.8%, p = 0.012)" and that synergies move it "markedly closer to the Vegas benchmark of 68.7%" (page 24). The phrase "better predictive accuracy than RAPM" does not occur verbatim in the PDF (the PDF has no abstract).
- "the one rigorous comparison of a player-level network against Pinnacle closing odds had the model behind the bookmaker (0.68 versus 0.70)" and "the one rigorous closing-line comparison lost 0.68 to 0.70" — now a second comparison exists: 67.1 against 68.7 percent on 2,235 games (page 23). The paper attributes the gap to information the market has: "oddsmakers incorporate non-performance information: injury updates, rest management, travel fatigue, lineup changes, and market movements that are unavailable in our retrospective player-only model" (page 24).
- "the papers that do touch forecasting all land in the same mid-60s band this application's model occupies, and none of them tried to beat a closing line" — corrected: this one did and trailed it by 1.6 points.
- "Player-level, possession-level and machine-learning models cluster in the same 66-74 percent accuracy band as Elo" — confirmed by this paper (66.3-67.1 percent).

Notes:

- "temporal-difference learning on a distributional win-probability model plus neural Shapley attribution" — confirmed (pages 1, 4-8: 81-bin categorical margin distribution, γ = 0.997 per second, 57-feature state encoder, 8-head attention attributor).
- "claiming better predictive accuracy than RAPM, more stability and better identification of defensive value and synergies" — confirmed in substance with numbers: accuracy above; year-over-year stability ρ = 0.76 against 0.71 for RAPM (page 13); Spearman ρ = 0.68 with RAPM and 0.54 with BPM (page 18); 15 percent of top-quintile defensive players sit in the bottom half of BPM (page 19); 127 significant synergies and 89 anti-synergies before FDR correction, 89 and 61 after (page 20).
- "(author listed as Ben Jenkins, Florida Atlantic University)" — not in the PDF.
- KQ2: "no numbers in the abstract snippet" — numbers now available (Table 8, page 23; Table 2, page 11).

### (3) Figures in the paper the report does not yet use

- Vegas closing line: 68.7 percent accuracy and 10.42 margin RMSE on 2,235 games from 2020-21 to 2023-24 (page 23); this is a published market benchmark on recent seasons, in the same band as the 67.8-68.1 percent the report takes from GitHub sources.
- RAPM-based ratings: 64.8 percent and RMSE 11.24 (page 23), a published game-level accuracy for the Sill lineage.
- Pairwise synergies add 0.8 points of accuracy (66.3 to 67.1, page 23); teams with more positive lineup synergies outperform their expected record by 2.1 wins and top-quartile teams by 3.4 wins (pages 20-22).
- Stability: RAPM needs about 15,000 possessions or 45 games for rankings with ρ > 0.80 against full-season estimates, DRL-Shapley about 5,000 or 15 games (Table 3, page 13).
- Learned action values: offensive rebounds +2.31 and steals +1.83 against the conventional +1.0 (Table 4, page 15); clutch three-pointers +4.5 against +2.8 (page 15).
- Caveats: the paper is a non-peer-reviewed competition submission; synergy estimates are "associational, not causal" (page 23); Table 1 (page 10) also reports 66.3 percent for the hybrid architecture, the same number as Table 8.

### (4) Relevance

Directly relevant and the strongest Sloan evidence for the report's thesis: a player-level, lineup-aware model on 2020-24 reaches 66.3-67.1 percent and trails the closing line (68.7) by about 1.6 points, the same gap the report measures for team Elo, with the residual attributed to injury, rest and lineup news.

---

## 8. Rafique and Sanders 2026, "Modern Playoff Tournament-Design Analysis across Sports: A Bayesian Modeling and Advanced Simulation Study"

Local copy of Rafique, Sanders 2026, read on 2026-10-23 (18 pages; pages 12 and 18 are blank; pdfinfo: created 2026-03-04, metadata author "Benjamin Bohr").

### (1) Bibliographic facts

- Authors: Hassan Rafique and Shane Sanders, Department of Sport Analytics, Falk College of Sport, Syracuse University (page 1).
- Year and venue: no year, venue or abstract printed; a "42 Analytics" footer logo (the Sloan competition branding) on every page. Internal dating fixes 2026: "the 2025 Super Bowl" (page 1), the 2025-26 CFP rules (page 6), and "The simulation was conducted on Dec 7th, the day CFP announces the playoff bracket and the paper submission deadline" with a 2025 bracket prediction (page 17), i.e. 7 December 2025 for the March 2026 conference.
- Licence or copyright line: none.
- File name: correct as is (year confirmed).

### (2) Claims attributed to this paper

Report:

- "The only playoff simulation at Sloan is Rafique and Sanders' cross-sport tournament-design study, which pairs closed-form probabilities with Monte Carlo under Bradley-Terry and Thurstone-Mosteller strengths and mentions the NBA only through the Play-In as an example of a hybrid Page-style design" — confirmed: closed-form four-team win probabilities (q1(A) = p14(p23 p12 + p32 p13), page 3; proof page 17); "Using a Bradley-Terry statistical learning model, we define p_ij = s_i/(s_i + s_j) ... Monte-Carlo simulations verify the Theorem's results by simulating 1 million tournaments" (page 4); "Thurstone-Mosteller stochastic-transitivity model, p_ij = Φ(θ_i − θ_j + η)" for the 12-team CFP (page 6); the NBA appears only as "the NBA Conference-level Play-In Tournaments" and "NBA Play-In Tournament" listed among PPS adopters (pages 1, 2, 3, 15). For the "only" qualifier see the Oh Finals demonstration under paper 1.

Notes:

- "Year uncertain (2025 or 2026)" — resolved: 2026 (page 17 dating above).
- "Combines closed-form theoretical probability derivations with Monte Carlo simulation; team strength modeled with Bradley-Terry and Thurstone-Mosteller" — confirmed (pages 3-6).
- "finds the classic single-elimination final-four bracket optimal within its class but dominated by hybrid designs such as the Page Playoff System" — confirmed (Theorem, page 3: "PPS maximizes the probability of the best team winning the tournament, where A maximizes among classical designs A, B, and C"; "A and PPS are fair").
- "which 'has been gaining traction in major sports leagues such as the NBA (Play-In Tournament)'" — not in the PDF. The paper says "In recent years, the PPS has been implemented in prominent sports leagues around the world, including the IPL, PSL, and BPL cricket playoffs and the NBA Play-In Tournament" (page 2).
- "main application is the College Football Playoff" — confirmed (pages 5-15).
- "A 2026 search snippet describes a tournament-format paper running up to one million simulated realizations, which may be this paper (unconfirmed)" — confirmed: 1 million four-team tournaments (page 4); 50,000 for the 12-team CFP (page 7).
- KQ4: "Page Playoff System raises the probability the strongest team wins while keeping ordinal fairness; NBA Play-In cited as an example of the hybrid design" — confirmed (pages 3-5).
- KQ4 Gaps: "Year and full results (number of simulations, strength parameters, NBA-specific outputs) of Rafique-Sanders could not be confirmed" — resolved: strengths for the four-team study drawn from spaced uniform ranges U(0.9,1), U(0.75,0.85), U(0.6,0.7), U(0.5,0.575) (Figure 2, page 4) and overlapping ranges (Figure 3, page 5); CFP strengths sampled from a multivariate normal with equally spaced means (pages 6-7); there are no NBA data and no NBA-specific outputs.

### (3) Figures in the paper the report does not yet use

- With overlapping strength distributions (seeding errors), PPS keeps the two strongest teams' title probabilities separated from the two weakest, while under single elimination "the win probability distributions shift closer to one another", producing more upsets (page 5): a published illustration that strength uncertainty flattens title probabilities.
- CFP simulation (Figure 4, page 7): under 2025-26 straight seeding the No. 1 seed wins 22.7 percent (22.4 without home advantage in round 1); the 2024-25 conference-champion-bye rule produced a bimodal pattern with the No. 5 seed at 19.9 percent above seeds 2-4.
- A Glickman-Stern state-space rating fitted to 2014-24 FBS scores matched the committee's top four in 64 percent of 2015-23 cases (pages 13-14) and "predicts all the outcomes correctly" for the 11 games of the 2024 CFP with 53 percent confidence on the final (page 14), a sample too small to mean anything.

### (4) Relevance

Cross-sport and NBA-free; the only transferable point for an Elo Monte Carlo is its demonstration that sampling team strengths with uncertainty (rather than point estimates) changes title probabilities, which is the correlated-rating-error issue the report already raises.

---

## Corrections to apply

Each line gives the file, the exact old text, and the replacement. Old strings were checked to occur exactly once in the named file.

### report.md

1. OLD: `but none publishes a game-winner accuracy, log loss or Brier score against a betting line.` NEW: `and one of them, the 2026 deep-reinforcement-learning valuation paper, publishes a game-winner accuracy against the Vegas closing line (66.3 percent for its player ratings and 67.1 percent with lineup synergies against 68.7 percent for the closing spread on 2,235 held-out games from 2020-21 to 2023-24, its Table 8); no Sloan paper publishes a log loss or Brier score against a betting line.`
2. OLD: `and the one rigorous comparison of a player-level network against Pinnacle closing odds had the model behind the bookmaker (0.68 versus 0.70).` NEW: `and both comparisons of a player-level model against closing odds had the model behind the bookmaker (0.68 versus 0.70 for Hubáček's network against Pinnacle; 67.1 versus 68.7 percent for the 2026 Sloan deep-reinforcement-learning ratings against the Vegas line).`
3. OLD: `and concludes qualitatively that the approach "is appropriate for modeling a basketball match" and that "changes in the team lineup or the opponent's lineup significantly affect the dynamics of a match"` NEW: `and concludes that the approach "is appropriate for modeling a basketball match" and that "either changes in the team lineup or changes in the opponent team lineup significantly affects the dynamics of a match progression" (page 1)`
4. OLD: `no accuracy, log loss or market comparison surfaced in any snippet of the paper, its Semantic Scholar record or Keshri's Columbia dissertation, which extends the evaluation to 2013-16` NEW: `its only accuracy figures are at the team-season level, a within-sample R-squared of 0.92 and an out-of-sample R-squared of 0.87 between simulated and actual 2013-14 win percentages on a 70/30 split of the 1,230 regular-season games (page 5, Figure 3), with no game-level accuracy, log loss or market comparison, and a game is awarded to the team that wins the majority of repeated simulations rather than given a probability (page 5); Keshri's Columbia dissertation extends the evaluation to 2013-16`
5. OLD: `(Proceedings of the 12th SSAC, 2018, later Annals of Applied Statistics 2020) models every possession as a non-stationary Markov decision process with shot-clock-dependent transitions, combines lineup-specific MDPs into team-average MDPs, and simulates whole seasons at sub-second resolution` NEW: `(Proceedings of the 12th SSAC, 2018; the version read is the Annals of Applied Statistics 2020 article "Markov Decision Processes with Dynamic Transition Probabilities: An Analysis of Shooting Strategies in Basketball," 14(3), 1122-1145) models every play that ends in a shot or turnover (fouls, free throws and rebounds are excluded) as a non-stationary Markov decision process with shot-clock-dependent transitions, assumes transitions are "invariant to the lineup" so that the chains can be combined into a team-average MDP (page 5), and simulates whole seasons at sub-second resolution`
6. OLD: `which won the best non-academic paper prize and is the origin of RAPM, reports a test RMSE of **11.47 points (R-squared 0.169)** on game margins for one configuration and finds that "the benefit of additional years of data and the use of a minutes cutoff turns out to be modest once regularization is applied"` NEW: `which won the best non-academic paper prize and is the origin of RAPM (the paper itself calls the method "regularization (a.k.a. ridge regression)" and never uses the term RAPM), reports a test RMSE of **11.47 points (R-squared 0.169)** on the margins of the March-April 2008-09 games for its best configuration (three seasons of data, lambda 3000, 1,200-minute cutoff; page 6), against 12.58 for predicting the mean home margin of 3.61 points and 12.01 (R-squared 0.088) for unregularized APM (page 5), and finds that "When using regularization, the benefit of additional years of data and the use of a minutes cutoff turns out to be modest" (page 6)`
7. OLD: `Gong and Chen's 2023 positional plus-minus claims "improved prediction accuracy in player plus-minus when factoring in player positions" over 2015-16 to 2021-22 without a magnitude in the abstract` NEW: `Gong and Chen's 2023 positional plus-minus paper estimates position-level means and variances of offensive and defensive plus-minus with a hierarchical Bayesian model over 2015-16 to 2021-22 (2019-20 excluded) and contains no prediction-accuracy claim and no out-of-sample test; the phrase "improved prediction accuracy in player plus-minus when factoring in player positions" attributed to its abstract does not appear in the paper`
8. OLD: `and the 2026 deep-reinforcement-learning valuation paper by Jenkins trains on a distributional win-probability model and claims "better predictive accuracy than RAPM," again without numbers in the abstract` NEW: `and the 2026 deep-reinforcement-learning valuation paper by Jenkins (the competition PDF is anonymised as "Paper ID 137") trains on a distributional win-probability model and reports, on 2,235 held-out games from 2020-21 to 2023-24, a game-winner accuracy of 66.3 percent (67.1 percent with pairwise synergies) against 64.8 percent for RAPM-based ratings, 62.4 percent for BPM and **68.7 percent for the Vegas closing line**, with margin RMSEs of 10.89 (10.71), 11.24, 11.87 and 10.42 (its Table 8, page 23); the paper attributes the remaining gap to "injury updates, rest management, travel fatigue, lineup changes, and market movements" (page 24)`
9. OLD: `The only playoff simulation at Sloan is Rafique and Sanders' cross-sport tournament-design study` NEW: `The only playoff-format simulation at Sloan is Rafique and Sanders' 2026 cross-sport tournament-design study (Oh, Keshri and Iyengar also ran a best-of-seven simulation of the 2014 Finals, but only as a two-lineup demonstration, page 7)`
10. OLD: `The published record agrees. FiveThirtyEight set home court at 100 Elo` NEW: `The published record agrees. Maymin, Maymin and Shen's win-probability model on 2006-07 to 2009-10 play-by-play gives the home team a 62 percent chance at tip-off against an evenly matched opponent (International Journal of Sport Finance 2012, page 12 of the PDF), FiveThirtyEight set home court at 100 Elo`
11. OLD: `Building a player-level model to beat the market will not either: the strongest operator of one kept Elo at a third of the weight, and the one rigorous closing-line comparison lost 0.68 to 0.70.` NEW: `Building a player-level model to beat the market will not either: the strongest operator of one kept Elo at a third of the weight, and the two closing-line comparisons lost 0.68 to 0.70 and 67.1 to 68.7 percent.`
12. OLD: `the papers that do touch forecasting all land in the same mid-60s band this application's model occupies, and none of them tried to beat a closing line.` NEW: `the papers that do touch forecasting all land in the same mid-60s band this application's model occupies, and the one that measured itself against a closing line, the 2026 deep-reinforcement-learning paper at 67.1 against 68.7 percent, trailed it by the same 1.6-1.8 points that separate Elo from the market.`
13. OLD: `the two documents most worth reading in full before any of them is cited as settled are the Oh-Keshri-Iyengar PDF, for the one Sloan simulation accuracy that may exist, and the SportsOddsHistory pre-playoff favourites table` NEW: `the Oh-Keshri-Iyengar PDF has since been read and holds only a team-season win-percentage R-squared (0.87 out of sample), so the document most worth reading in full before it is cited as settled is the SportsOddsHistory pre-playoff favourites table`

### notes/sloan_papers.md

14. OLD: `I did not find, and therefore do not report, any Sloan NBA paper that publishes a game-outcome accuracy, log loss or Brier score against a betting-line baseline; that absence is itself a finding (see Gaps).` NEW: `On reading the PDFs, one Sloan NBA paper does publish a game-outcome accuracy against a betting-line baseline: the 2026 deep-reinforcement-learning valuation paper (Table 8, page 23: 66.3 percent, 67.1 with synergies, against 68.7 percent for the Vegas closing line on 2,235 games); none publishes a log loss or Brier score against a betting line.`
15. OLD: `and introduces ridge regression ("RAPM"), described as nearly doubling APM's accuracy.` NEW: `and introduces ridge regression (the paper calls it "regularization (a.k.a. ridge regression)" and does not use the term RAPM), described as nearly doubling APM's accuracy (test R-squared 0.088 to 0.169, pages 5-6).`
16. OLD: `"changes in the team lineup or the opponent's lineup significantly affect the dynamics of a match"; evaluated on 2013-14 NBA data.` NEW: `"either changes in the team lineup or changes in the opponent team lineup significantly affects the dynamics of a match progression" (page 1); evaluated on 2013-14 NBA data with 70 percent of the 1,230 regular-season games for training and 30 percent for testing; simulated against actual team win percentages R-squared 0.92 within sample and 0.87 out of sample (page 5, Figure 3), player points per game R-squared 0.74 and 0.63 (page 6); a game is awarded to the team that wins more of the repeated simulations (page 5), 101 per game in the 2014 Finals demonstration, which also runs a best-of-seven under two Spurs lineup weightings (page 7).`
17. OLD: `(abstract snippet; the site misspells "Baskeball")` NEW: `(the PDF's own text layer carries the same "Baskeball" misspelling in the title, page 1)`
18. OLD: `Models each possession as a non-stationary Markov decision process with shot-clock-dependent transitions estimated by Bayesian hierarchical models; lineup-specific MDPs are combined into team-average MDPs;` NEW: `Models each play ending in a shot or turnover (fouls, free throws and rebounds excluded, pages 11 and 14) as a non-stationary Markov decision process with shot-clock-dependent transitions estimated by Bayesian hierarchical models; because "most teams use upward of 500 lineups over the course of a season," transitions, rewards and policies are assumed "invariant to the lineup" and the chains are combined into a team-average MDP (page 5); the repository PDF is the Annals of Applied Statistics version and carries "© Institute of Mathematical Statistics, 2020" on page 1;`
19. OLD: `conclusion is qualitative ("the graphical model approach is appropriate for modeling a basketball match"); no accuracy, RMSE, log loss or market comparison appeared in any snippet of the paper page, the Semantic Scholar record or the dissertation.` NEW: `the paper reports team-season win-percentage R-squared of 0.92 within sample and 0.87 out of sample on a 70/30 split of the 2013-14 games (page 5) and says this "is comparable to Shirley [1] and Štrumbelj and Vračar [2]" (page 6); it reports no game-level accuracy, RMSE, log loss or market comparison.`
20. OLD: `"improved prediction accuracy in player plus-minus when factoring in player positions" over 2015-16 to 2021-22; magnitude and metric not in the abstract snippet.` NEW: `no prediction-accuracy claim, metric or out-of-sample test appears in the paper (an anonymised competition PDF, submission 111280, without abstract or author names); its outputs are posterior means and variances of positional offensive and defensive plus-minus for 2015-16 to 2021-22 with 2019-20 excluded (Tables 6 and 7, pages 7-8).`
21. OLD: `Hierarchical Bayesian linear model of plus-minus at the position level, 2015-16 through 2021-22; reports "improved prediction accuracy in player plus-minus when factoring in player positions."` NEW: `Hierarchical Bayesian linear model of plus-minus at the position level, 2015-16 through 2021-22 with 2019-20 excluded (page 4); the PDF contains no prediction-accuracy claim and no out-of-sample test, and the phrase "improved prediction accuracy in player plus-minus when factoring in player positions" is not in the paper.`
22. OLD: `claims "better predictive accuracy than RAPM" and more stability; no numbers in the abstract snippet.` NEW: `Table 8 (page 23): win prediction on 2,235 test games, Vegas closing line 68.7 percent (margin RMSE 10.42), BPM-based 62.4 (11.87), RAPM-based 64.8 (11.24), DRL-Shapley 66.3 (10.89), DRL-Shapley plus synergy 67.1 (10.71); in-game win-probability Brier 0.168 against 0.198 for logistic regression (Table 2, page 11); year-over-year stability rho 0.76 against 0.71 for RAPM (page 13). The paper attributes the gap to the line to "injury updates, rest management, travel fatigue, lineup changes, and market movements" (page 24).`
23. OLD: `claiming better predictive accuracy than RAPM, more stability and better identification of defensive value and synergies.` NEW: `reporting game-winner accuracy on 2,235 held-out games of 2020-21 to 2023-24 of 66.3 percent (67.1 with synergies) against 64.8 for RAPM, 62.4 for BPM and 68.7 for the Vegas closing line (Table 8, page 23), year-over-year stability rho 0.76 against 0.71 for RAPM (page 13), rank correlation 0.68 with RAPM (page 18) and 127 significant player synergies (page 20). The competition PDF is anonymised ("Paper ID 137"), so the author attribution rests on the event page.`
24. OLD: `Only a handful of Sloan basketball papers publish forecast-accuracy numbers at all, and none that I could find publish game-winner accuracy, log loss or Brier score against a betting-market baseline;` NEW: `Only a handful of Sloan basketball papers publish forecast-accuracy numbers at all, and one, the 2026 deep-reinforcement-learning paper, publishes game-winner accuracy against the Vegas closing line (67.1 against 68.7 percent on 2,235 games); none publishes a log loss or Brier score against a betting-market baseline;`
25. OLD: `Whether Sill (2010) compared against Vegas spreads is unknown (no snippet mentioned it).` NEW: `Sill (2010) did not compare against Vegas spreads; his only baseline is predicting the mean home margin of 3.61 points (RMSE 12.58, page 5).`
26. OLD: `No Sloan NBA paper's exact test-set size, log loss, Brier score or calibration plot could be extracted because PDFs were unreachable; the Oh-Keshri-Iyengar PDF and Keshri dissertation are the top priority to read for a possession-simulation accuracy number.` NEW: `Test-set sizes read from the PDFs: Sill, the March-April games of 2008-09; Oh-Keshri-Iyengar, 30 percent of the 1,230 games of 2013-14; the 2026 deep-RL paper, 2,235 games. The Oh-Keshri-Iyengar accuracy is a team-season win-percentage R-squared (0.87 out of sample), not a possession- or game-level number; no Sloan paper gives a calibration plot of game forecasts.`
27. OLD: `Lineups: lineup-specific MDPs combined into team-average MDPs via a transition weighting scheme; Bayesian hierarchical models borrow strength across players and time; uncertainty propagated via posterior samples.` NEW: `Lineups: the model assumes transitions, rewards and shot policies are "invariant to the lineup, that is, other players do not impact transitions and rewards" (page 5) so that a team-average MDP can be built via a transition weighting scheme; it is not evidence that lineups matter; Bayesian hierarchical models borrow strength across players and time; uncertainty propagated via posterior samples.`
28. OLD: `Positions: including player positions improves prediction of player plus-minus; versatile offensive players and paint-defending bigs are the most valuable contributors.` NEW: `Positions: Versatile Scorers have the highest offensive plus-minus and Bigs the highest defensive plus-minus in every season (pages 8-10); the paper makes no claim about prediction.`
29. OLD: `**Year uncertain (2025 or 2026) - Hassan Rafique and Shane Sanders,` NEW: `**2026 - Hassan Rafique and Shane Sanders,`
30. OLD: `which "has been gaining traction in major sports leagues such as the NBA (Play-In Tournament)"; main application is the College Football Playoff.` NEW: `of which the paper says "In recent years, the PPS has been implemented in prominent sports leagues around the world, including the IPL, PSL, and BPL cricket playoffs and the NBA Play-In Tournament" (page 2); main application is the College Football Playoff.`
31. OLD: `A 2026 search snippet describes a tournament-format paper running up to one million simulated realizations, which may be this paper (unconfirmed).` NEW: `The paper verifies its theorem by simulating 1 million four-team tournaments (page 4) and the 12-team College Football Playoff with 50,000 simulated tournaments (page 7); its 2025 bracket simulation was run on 7 December 2025, "the paper submission deadline" (page 17), which fixes the conference year as 2026. The PDF prints no year, venue or abstract.`
32. OLD: `Year and full results (number of simulations, strength parameters, NBA-specific outputs) of Rafique-Sanders could not be confirmed.` NEW: `Rafique-Sanders (2026) run 1 million four-team simulations with Bradley-Terry strengths drawn from uniform ranges (Figures 2 and 3, pages 4-5) and 50,000 12-team CFP simulations under Thurstone-Mosteller strengths (page 7); the paper contains no NBA data and no NBA-specific outputs.`
33. OLD: `I found no evidence of a Sloan paper on NBA playoff-series probabilities or championship odds specifically; the Rafique-Sanders paper is the only playoff-simulation paper and it is cross-sport.` NEW: `I found no evidence of a Sloan paper on NBA playoff-series probabilities or championship odds specifically; the Rafique-Sanders paper is the only playoff-format simulation paper and it is cross-sport, and Oh, Keshri and Iyengar's best-of-seven simulation of the 2014 Finals (Figure 6, page 7) is a two-lineup demonstration, not a forecast.`
34. OLD: `Uses NBA play-by-play data and a win-probability method (borrowed from financial economics / option value) to derive the "Q+1" rule for benching a starter in foul trouble; the authors say the win-probability method can be applied to other problems.` NEW: `Uses four seasons (2006-07 to 2009-10, 4,723 games) of NBA play-by-play data and a win-probability method (Stern's Brownian-motion model with a jump-diffusion variance term and option-value reasoning from finance) to test the "Q+1" benching rule that coaches already apply after 72 percent of threshold fouls, finding it generally correct (pages 3, 5, 11-13); the model gives the home team a 62 percent tip-off win probability against an evenly matched opponent (page 12); the authors say the win-probability method can be applied to other problems. The local copy read on 2026-10-23 is the journal version and carries "© 2012 West Virginia University" on page 1.`

### research/papers/README.md (file-name and licence notes; the README is a repository file, so these are proposals)

35. Cite the anonymised 2023 positional plus-minus submission as Gong, Chen 2023, and record that the PDF is an anonymised competition submission (ID 111280) whose author names come from the Sloan listing.
36. Cite the anonymised deep-reinforcement-learning submission as Jenkins 2026, and record that the PDF is anonymised ("Paper ID 137") and that the author name comes from the Sloan event page.
37. OLD: `3. Sandholtz, Bornn 2018, Replaying the NBA. MIT Sloan; arXiv 1812.05170 (check the licence) and Annals of Applied Statistics 2020 (paywalled: link only); the authors' site offers a PDF.` NEW: `3. Sandholtz, Bornn 2018, Replaying the NBA. MIT Sloan; arXiv 1812.05170 (check the licence) and Annals of Applied Statistics 2020 ("Markov Decision Processes with Dynamic Transition Probabilities: An Analysis of Shooting Strategies in Basketball," 14(3), 1122-1145; the file in this directory is that version and carries the Institute of Mathematical Statistics copyright line, so confirm the author-site copy is the permitted source or keep the link only).`
38. Add to the reading list: the Maymin, Maymin, Shen PDF is the International Journal of Sport Finance 2012 version with a West Virginia University copyright line; confirm redistribution is allowed or keep the link only.
