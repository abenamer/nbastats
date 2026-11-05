---
type: Concept
title: YAXEL, the emergence term
description: How a player's recent box scores, set against the worth he entered the season with, could move his team's rating before the results do, and how the rule is backtested over the stored seasons before it is used.
tags: [yaxel, worth, box-score, backtest, rating]
generated: { by: "human:abenamer", at: "2026-11-04T09:00:00Z" }
sources:
  - id: domain
    resource: /src/domain/yaxel.rs
    title: The rule and its scoring
  - id: app
    resource: /src/application/yaxel.rs
    title: The samples and the report
---

# Purpose

The team rating learns from results, at a pace of about 25 games for half of a step change. A rookie, or a player who changed, can play like a star for a week while the rating still carries last season's view of his team. YAXEL is the term that lets his box scores in: a shift to his team's rating from what his recent lines say beyond the worth he entered the season with. It is named for the rookie whose first weeks the market priced and the rating did not. See [ADR 0014](/decisions/0014-yaxel-an-emergence-term-backtested-before-it-is-used.md).

# The rule

For a game, each team's floor is the players who played in the team's previous game. For each of them:

* The entering worth is the worth the model priced him at coming into the season: from the games his team played without him and his box scores in the seasons before, or for a rookie his draft bracket's mean scaled by the box calibration ([Player worth](/concepts/player-worth.md)). A player with no estimate, a rookie whose bracket prices as nothing among them, enters at 0, the replacement level. Nothing from the season itself.
* The recent box worth is his box worth over the window of days before the game: the box-score table built from the league's games in that window alone, his lines pooled by minutes, scaled by the box calibration to the worth scale, as the what-if window is ([Season forecast engine](/concepts/season-engine.md)).
* The surprise is the recent box worth shrunk toward the entering worth by his minutes in the window, less the entering worth: `(recent − entering) × minutes / (minutes + prior minutes)`. At the prior minutes the two weigh the same; with no minutes there is no surprise.

The team's shift is the damping times the sum of its floor's surprises. The forecast of the game is the Elo forecast with each team's shift added to its pre-game rating, the home bonus as the season's fitted one.

Three parameters: the window in days, the prior minutes and the damping. None is set by hand; the backtest chooses them.

# The backtest

`nba-elo-simulator yaxel-backtest --from 2010 --to 2026` samples every season in the range that has at least 100 played regular-season games, box scores with BPM and an earlier season to price the players from (a season without is listed as skipped, with the reason). For each game it keeps the stored pre-game ratings, the result, each team's game number and, per window of the grid, the two floors. Then `domain::yaxel::backtest` scores the rule:

* The grid is windows of 7, 14 and 28 days, prior minutes of 100, 250 and 500, dampings of 0.25, 0.5, 0.75 and 1.
* The seasons in order are split into two folds by turns. Each fold's parameters are the grid point with the greatest mean Brier gain, base Brier less YAXEL Brier over all games of its training seasons; the fold's test seasons are scored under them. So no season is scored under parameters chosen on it.
* The scores are the mean Brier score and log loss of each forecast, the mean gain, its standard error and the gain in standard errors, over all games, over the early season (games in which neither team has played more than thirty; `--through` sets the cut) and over the games with a new player on a floor.
* The grid is run twice: with the term on every player of the floor, and with the term on the new players alone, so the veterans' surprises, noise when their entering worth is right, do not drown a rookie's.
* The report prints, for each run, each fold's choice and test score, every test season under its fold's parameters, and the test seasons pooled; `--json <path>` writes it whole.
* Each game's sample also carries each side's strength as the live forecast would read it from the box scores: its full-strength pre-game rating and the Elo it is missing tonight, read from its previous game and from tonight's box score (see The absence-penalized rating below). No rule uses them; the premise is read against them.

## New players

A new player is one with no line in any stored season before the one played, whatever his draft year: a first-year player, one who lost his first season, or an arrival from abroad; a draft older than five years before the season is no new player's, so a gap in the stored box scores does not turn a veteran into one (the same rule prices a rookie, [Player worth](/concepts/player-worth.md)). The rating has no record of him, and his entering worth is his draft bracket or the replacement level. A game is one with a new player on a floor when a new player played at least 24 minutes (`--new-minutes` sets it) in his team's previous game, the game that put him on the floor. The first stored season of box scores cannot tell a new player, since nobody has an earlier line and every player drafted within five years, or without a draft year, would look new (the test is for a season of box scores among the three before, the ones the players are priced from): the report says so, counts none of its games as one with a new player on a floor, and leaves the season out of the pooled cut and of the run on the new players alone.

Before any rule, the report measures the premise on Elo's own forecasts, season by season and pooled, all games and the early season: the share of games with a new player on a floor, Elo's Brier score on them against the rest, and the bias of the new players' teams, each team-game's result less Elo's chance (over the games with a new player on one floor only: with one on both, the two residuals would cancel), in points of win chance, as Elo at even odds (a point is about 7 Elo, the slope of the logistic at a difference of zero) and in standard errors. A positive bias says those teams beat the rating; a bias over two standard errors says the rating is slow on them. Then both runs of the grid are scored on those games too: for the term on the new players alone, they are the games where it can act, and its gain there says whether it does.

## The absence-penalized rating

The same bias is read against the rating the Tonight page prices at: the full-strength rating less tonight's absences ([Player worth](/concepts/player-worth.md), Full-strength rating). The page reads both from the injury report; the stored seasons before the feed have no reports, so the backtest reads them from the box scores. A player is on a team's roster from his first game for it, or from the season's start when he ended the season before with the team (the team of his last line in it), to the day before his first game for another team, or the season's end: a player out from the first game who returns is missing until he does, a player signed in season is not, a player traded away leaves the roster when he plays for his new team, and a player who never plays for the team is not on it. A roster player with no line played sat the game; a box score tells who sat only when at least five of the team's players played, since fewer is a box score loaded in part. Each is priced at the worth he entered the season with, as the floors are, at the expected penalty, a side capped at 400 Elo. The season's played games are replayed with those absences out of each game's expectation, as the full-strength rating is (`domain::full_strength`), which gives each game's full-strength pre-game rating: the stored pre-game rating moved by the gap between the replay with the absences and the replay without them, held within 400 Elo as a forecast holds it, which is the page's own full-strength rating when the stored chain is the replay, as the computed ratings are ([ADR 0004](/decisions/0004-compute-first.md)). Tonight's missing Elo then comes off, read two ways: from the team's previous game, the players on its roster at both games who sat it, the lineup the backtest can know before tip-off on a roster read from the season's lines as a report would list it; and from tonight's box score, the players who sat tonight, which the report tells the live page before the game for the players it lists out.

The report prints, season by season and pooled, all games and the early season: the team-games the bias is read over, the Elo missing tonight on the new players' teams and on the rest, the bias against Elo and against the two penalized ratings, and the Brier score of the three forecasts over all games, as a check that the penalized rating forecasts no worse than Elo. A bias that is gone against the penalized rating is the absence effect the live page already prices; one that stays is not. The run over the stored history (18 seasons, 2026-11-05) read the bias of the new players' teams at −13 and −14 Elo against the two penalized ratings, from −16 against Elo, at four standard errors: the absences account for about 2 to 3 Elo of the gap, and the rest stays; [ADR 0014](/decisions/0014-yaxel-an-emergence-term-backtested-before-it-is-used.md) records the run.

The bar for taking a rule to a page is a gain of two standard errors on the pooled test seasons and on the early season. The backtest writes nothing and changes no rating or page. The deciding runs over the stored history (17 seasons, 2026-11-05) did not clear the bar for the term on every player or for the term on the new players alone, so the live term is off. The new-player cut inverted the premise: the teams that play a new player heavily fall short of Elo's chance by about 16 Elo at even odds, in every season, so the rating of a team leaning on a rookie is too high, not too low; [ADR 0014](/decisions/0014-yaxel-an-emergence-term-backtested-before-it-is-used.md) records the runs and the reading.

# Limits

* The backtest's floor is the previous game's players, so an absence tonight is not known to it; the live term would read the injury report. The backtest is a little harder on the rule than the page would be.
* The absence-penalized rating reads the absences from the box scores, not from a report. A coach's decision, a rest or a suspension read as an injury, as the live model's own rule for a regular player absent from the box scores does. The roster is read from the season's lines with hindsight on who was on the team, as a report lists it: a player who never plays for the team in the season is not on its roster, so a season-long absence is never missing; a player out from the first game who returns is missing from the start when he ended the season before with the team, and from his first game when he joined in the summer; a player waived stays missing to the season's end. A season with stored reports would read closer to the page; the backtest does not use them, so every season is read the same way.
* After a few weeks a player's effect is in the results and in his surprise alike; the damping is the only guard against counting it twice, and a decay with games played is the refinement if the gain is there.
* A rookie's entering worth is his draft bracket, so a rookie's surprise is everything his box scores show above it, with six games' worth of noise. The prior minutes set how much of that counts.
* The window table is built from a few days of games, so its team adjustment is rough; the window and the calibration carry that.
* The gain in standard errors treats the games as independent. Games of one date share a window table and a team's games follow one another, so the error is a little understated and the bar of two leaves room for that.
* The numbers in the report depend on the store they are run against; the ones in this bundle are the runs recorded in the decision record, the deciding one on the maintainer's store.
