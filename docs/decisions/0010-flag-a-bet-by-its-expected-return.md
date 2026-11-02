---
type: Architecture Decision
title: Flag a bet by its expected return at the posted price
description: Why the Tonight panel marks a bet when the model's chance, taken at the sportsbook's own price, returns at least a chosen share of the stake, why spreads use a measured scatter of margins, and why the record scores the flagged bets apart.
tags: [decision, market, odds, edge]
generated: { by: "human:abenamer", at: "2026-11-02T14:00:00Z" }
---

# ADR 0010 — Flag a bet by its expected return at the posted price

## Context

The Tonight panel sets the model's chances beside the sportsbook's ([ADR 0009](0009-market-lines-from-the-odds-api.md)) and shows the gap in percentage points. The maintainer asked the panel to say which bets the model likes and why, and set the bar at five percent above the bookmaker's margin. A gap in chances is not that: five points on a −900 favourite are worth nothing at that price, five points on a +250 underdog are worth a lot, and a spread bet has no chance column at all.

## Decision

* `domain::market` reads the model's chance as a bet. The expected return of a one-unit bet is the chance of winning times the payout at the posted price, less the chance of losing; a push returns the stake and counts for nothing. The posted price carries the bookmaker's margin, so a positive return is already above the margin, and the bar (`min_return`, five percent of the stake by default) is on top of it. The rule has no other constant.
* A spread bet's chance of covering comes from the model's expected margin and the scatter of real margins around expected ones: a normal distribution over whole points, with a push at a whole-number line; a game cannot end level, so the normal's mass at a margin of zero is taken out and the rest rescaled. The scatter is measured, not assumed: the standard deviation of the residuals (actual home margin less the margin the pre-game ratings expected) over the played regular-season games of the season and the seasons of its home-bonus window, about 12 points; under 200 games the Cup engine's 12 stands in.
* `edges` lists, best first, the bets on a game that reach the bar: each side's moneyline and each side of the spread. The market carries them per game, with the bar and the scatter; `GET /api/seasons/{season}/tonight/market?min_return=` sets the bar from 0 (any positive return) to 1. The panel marks the sportsbook's cell each flagged bet is on and lists them under the table with the arithmetic in words, the bar, and the standing caveat: the edge is the model's disagreement with the sportsbook, unproven until the record says otherwise.
* The record scores the flagged bets apart from the lean and the spread side: `score` takes the bar and the scatter, applies the same rule to each stored line, and scores each flagged bet at its price. The summaries gain the flagged record (won, lost, push, units, return per bet), so the bets the panel marks are the bets the record judges. The rule is applied when the record is read, not when the line is stored, so a change of bar re-reads the whole record at the new bar.

## Consequences

* A reader sees the bets and the reason in one place, in the units a bettor uses: units per unit staked at the price on offer.
* The bar is the reader's. The default is the maintainer's five percent; the field on the panel moves it, and the record follows the field.
* The spread chances rest on one measured number and a normal shape; a fatter-tailed or skewed scatter would move chances near the line by a point or two. The panel says what the scatter is and how many games it rests on.
* The flagged record is the test of the rule. Until it holds a few hundred bets, the highlights are a hypothesis, and the panel says so.

## Alternatives

* Flagging on the gap in chances (model less market) was rejected: it ignores the price, so it marks the wrong bets.
* A fixed bar in the code was rejected: the bar is a preference, so it is a field, with the default recorded here.
* Kelly stakes were not added: a stake size needs a bankroll and a confidence in the model's chances that the record has not yet earned. The expected return is the input a Kelly stake would use, and can be added when the record supports it.
* Storing the flags with each snapshot was rejected: the rule and the bar may change, and the stored line holds everything the rule needs.
