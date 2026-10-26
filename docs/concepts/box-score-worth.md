---
type: Model
title: Box-score worth
description: How the application values a player from his box score with Box Plus/Minus 2.0, VORP and PER, and how it puts that value on the Elo scale beside the worth read from the games he missed.
tags: [players, worth, box-score, bpm, vorp, per, elo]
generated: { by: "human:abenamer", at: "2026-10-26T12:00:00Z" }
sources:
  - id: code
    resource: /src/domain/bpm.rs
    title: Box-score worth module
  - id: worth
    resource: /docs/concepts/player-worth.md
    title: Player worth and availability
---

# Purpose

[Player worth](player-worth.md) reads what a team loses in the games a player missed. A player who never sits has no such reading. The box score gives a second estimate for every player who played: what his production says he is worth. The Players page shows the two side by side, in the same Elo unit. The gap between them is itself a reading.

The method is Daniel Myers's published Box Plus/Minus 2.0, the metric behind Basketball-Reference's BPM and VORP columns. The application computes it from its own stored box scores. It does not fetch the published values.

# Inputs

The regular-season games of the season with box-score lines for both teams. A game with lines for one team only is left out and counted. The lines give each player's minutes, points, rebounds (offensive and defensive), assists, steals, blocks, turnovers, fouls and shooting. The players' listed positions come from the player records.

# Measure

1. **Possessions.** Each game's possessions follow Basketball-Reference's estimate. It uses both teams' field goal attempts, free throw attempts, offensive rebounds, misses and turnovers. Every player statistic becomes a rate per 100 team possessions while he was on the floor. His share of the floor is his minutes over a fifth of the team's minutes.
2. **Position (1 to 5).** A regression on the player's shares of his team's rebounds, steals, fouls, assists and blocks, with the published coefficients. The estimate is blended with 50 minutes of the listed position and limited to 1 to 5. One constant per team sets the team's minutes-weighted mean to 3.
3. **Offensive role (1 creator to 5 receiver).** A regression on the shares of the team's assists and of its threshold points. Threshold points are points above a shooting efficiency 0.33 points per true shot attempt below the team's. The estimate is blended with 50 minutes at role 4, limited and centred the same way.
4. **Raw BPM.** The per-100 statistics weighted by the published coefficients. In the full regression, assists, rebounds, steals and blocks vary linearly with the position. Shot attempts vary with the offensive role. Points, threes, turnovers and fouls do not vary. In the offensive regression turnovers vary with the position and assists do not. Points are first adjusted for the team's shooting context. A position constant penalises guards, and a role constant lifts receivers.
5. **Team adjustment.** The team's players, weighted by their shares of the floor, sum to the team's adjusted efficiency. That is its margin per 100 possessions, adjusted for the schedule and for the lead it played with. One constant per team closes the gap. The offensive half is closed the same way, and defensive BPM is the difference.
6. **VORP.** BPM above the replacement level of -2.0, times the share of the floor, times the team's games over 82. Wins over replacement are VORP times 2.7.
7. **PER.** John Hollinger's Player Efficiency Rating, pace-adjusted, with the league's minutes-weighted mean scaled to 15. It is shown for recognition and used for nothing.

The raw-BPM step of the published worked example (a 2017 season at position 2.3 and role 1.0) reproduces within the rounding of its own lines. That checks the coefficient tables and the interpolation. The shooting adjustment and the team adjustment of the example cannot be checked, because the page does not give their inputs.

# The Elo scale

The box worth is what the team gains over a replacement player in the games he plays. It is BPM above replacement, times his share of the floor in those games, times the team's possessions per game over 100. Times 28 Elo per point, it is on the scale of the player worth, which is per game of absence. A player who missed games is not diluted, so the roster's box worths do not sum to the team.

The VORPs do. A team's VORPs sum to its adjusted efficiency plus 10 points per 100 possessions (five replacement players at -2.0), scaled by its games over 82. The VORP column is therefore a pro-rata split of the team's margin over a replacement-level team.

The box worth and the player worth measure different things with the same unit. The box worth is production. The player worth is the team's result without him. A player whose two figures agree is well read. Where they disagree, the box score may miss his defence, or the games he missed may have fallen against weak opponents.

# What the application chose

The published method leaves a few steps to the implementer. The application's choices:

* **Shooting baseline.** The points adjustment compares the team's points per true shot attempt with the baseline of the regression, which is not published. The application uses the league average of the season. The baseline moves credit within a team. A higher baseline lifts the high-usage players and lowers the low-usage ones. For a star with 28 true shot attempts per 100 possessions, the move is about one BPM point per 0.12 points per attempt. The published example implies a lower baseline than the league average, so the application's high-usage players read lower than Basketball-Reference's.
* **Offensive target.** The offensive BPMs of a team sum to its schedule-adjusted offensive rating plus half the lead effect. The page does not say what they sum to.
* **Schedule adjustment.** Each team's offence is rated against the defences it met, and its defence against the offences. The two are iterated to a fixed point over the covered games. Basketball-Reference adjusts with its own team ratings.
* **Average lead.** The lead a team played with is half its mean margin. That is what a lead that grows evenly from zero to the final margin gives. The published example estimates it from the play-by-play.
* **Season level.** The regression runs on season totals, as the published method did before 1985. Basketball-Reference sums game-level values from 1985 on, which handles the schedule and missed games better.
* **Listed position.** The source lists G, F, C or a pair such as G-F. The application reads PG as 1, SG 2, G 1.5, SF 3, PF 4, F 3.5, C 5, and averages a pair. A player without a label leans on 3.
* **Missing turnovers.** A player's turnovers are imputed by the published 1974-77 estimator at an assumed age of 27, because the store has no birth dates. The possession estimate of a game then assumes 0.17 turnovers per true shot attempt for each team.
* **Missing rebound split.** The possession estimate assumes an offensive rebound share of 0.3. BPM is not computed in that case.
* **Team rebounds.** Team totals are the sums of the players' lines. Rebounds the box score credits to the team, not to a player, are not in them.

Because of these choices the figures are close to Basketball-Reference's but not identical, and the application does not claim them as such.

# Coverage by era

| Statistics in the box score | What is computed |
|-----------------------------|------------------|
| Minutes, rebound split, steals, blocks, turnovers | BPM, OBPM, DBPM, VORP, wins, box worth, PER |
| The same without turnovers | The same, with turnovers imputed; the page says so |
| Minutes without steals, blocks or the rebound split | PER only, with the missing statistics as zero for everyone; the page says so. The position estimate uses the published pre-1974 coefficients, and shows in the Pos column only |
| No minutes | Nothing |

A statistic counts as present when more than half of the season's lines carry it. A line without it then enters as zero for that player. A team whose lines carry no minutes while the league's do has no players in the table, and the note counts such teams.

The league recorded steals, blocks and the rebound split from 1973-74, turnovers from 1977-78, and minutes from 1951-52. Whether a season qualifies depends on what the source delivered for it. The table reports its coverage.

# Limits

* The box score misses most of defence. Defensive BPM carries the position constant and little else; read it as a hint.
* A low-minutes player's estimates lean on the listed position and the role prior, by design. His VORP is small either way.
* A traded player has one row per team. His box worths are each per game played with that team. The player worth is one figure across teams.
* The coefficients were fitted on 1997-2016 NBA play. In earlier eras and in the ABA they are borrowed.
* Nothing here changes a rating or the forecast. The injury report is still priced with the player worth.
* The first request for a season computes its table, and the first request for a player's career computes every season of it. Both are cached until the next import or sync.
* On 2026-10-26 the method was tested on synthetic seasons and the published example only: the sandbox holds no box scores.
