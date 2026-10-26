---
type: Model
title: Return from absence
description: How the application reads what a player delivers after a run of missed games against what he delivered before, from the stored box scores, grouped by his quality and the run's length and corrected with a matched control; a report that changes no forecast.
tags: [players, worth, box-score, bpm, injuries, return, report]
generated: { by: "human:abenamer", at: "2026-10-26T18:00:00Z" }
sources:
  - id: code
    resource: /src/domain/recovery.rs
    title: Return-from-absence module
  - id: box
    resource: /docs/concepts/box-score-worth.md
    title: Box-score worth
  - id: worth
    resource: /docs/concepts/player-worth.md
    title: Player worth and availability
---

# Purpose

The [season forecast](season-engine.md) prices a player who is out at his full worth until his return date, and at nothing from that date. A player who comes back from a long absence often plays fewer minutes for a while, and may play worse. The forecast does not know by how much, for how long, or whether a star and a bench player differ. Nobody does: the papers in the project's reading list name injuries as an omitted variable and go no further.

This report reads the answer from the application's own data. It is a measurement, not a rule. Nothing in it changes a rating, a penalty or a forecast. If its curves hold up out of sample, a later change can carry them into the engine; if they do not, the report stays as the record of what was tried.

# Inputs

The regular-season games of each season in the range with box-score lines, and the season's [box-score table](box-score-worth.md), which gives every player his estimated position and offensive role and every team its pace and team adjustment. A season whose lines do not support Box Plus/Minus (no rebound split, steals or blocks) is skipped and named.

The game logs carry no injury reports before the present. The report does not need them. An injury leaves a footprint: a player who was playing disappears from his team's lines for a run of games and comes back.

# Measure

1. **Team games.** For each team, its regular-season games with lines, in schedule order. A player's status in each is played (minutes above zero) or not. A line with no minutes counts as not played.
2. **Return event.** A run of five or more team games not played, with a played game before and after, by the same team. The run must have twenty team games before it and ten after the return. In the twenty before, the player must have played fifteen games, at fifteen minutes a game on average: a regular player. Those twenty games must hold no run of five, as a control stretch must not: a run inside them would make the "before" a return of its own. A run that reaches the end of the season is not an event (no return). A player traded away is not an event with his old team (no return) and has no window before with his new one.
3. **Windows.** Three windows of team games: the twenty before the run, the first ten from the return (the early half) and the next ten (the late half; shorter when the season ends, absent when it has ended). In each, over the games he played: his minutes per game, his share of the floor, his BPM and his box worth per game played in Elo, read as the box-score worth reads a season but over the window's lines, with the season's position, role and team adjustment. Also his availability: games played over team games.
4. **Brackets and lengths.** Each event is placed by the player's BPM in the window before the run (below 0, 0 to 2, 2 to 5, 5 and up) and by the run's length (5 to 9, 10 to 19, 20 to 39, 40 and more team games). A player who returns worth more delivers more Elo in absolute terms by construction; the brackets ask whether the shape of the return differs by quality.
5. **Control.** A player whose twenty games before the run were strong was partly good and partly lucky, and his next games would be weaker with no injury at all. That fall is largest in the top bracket, exactly where it would be read as injury damage. To take it off, the report reads every stretch of forty team games of a regular player that holds no run of five and lies clear of the windows around any run of his: the same three windows, and the same bracket rule on the first twenty games. The stretches of one player start every ten games. Each bracket's control drop is the mean over its samples. A matched control of the same player around his own run was tried first and dropped: an 82-game season almost never holds forty clean games beside a run's windows.
6. **Cells.** For the whole range, for each bracket, for each length and for each bracket by length: the number of events, the mean worth, BPM, minutes and share in each window, the availability after the return, the drop (early worth minus worth before, with the standard error of its mean), the drop in the late half, the number of control samples in the brackets of the cell's events, the control drop (each event taking its own bracket's mean), and the effect: the mean over the events of the event's drop minus its bracket's control drop, with an error that combines the events' and the controls'. A cell whose bracket has fewer than two control samples has no effect, since the control's own error is then unknown.

# Reading the report

The drop is in Elo per game played. A drop of 14 is about half a point of margin per game. A mean over a few events says little; the standard error beside it says how little. The effect is the figure to read for a rule: it is what the return adds to the fall two windows show anyway. A negative effect in the early half that fades in the late half is the ramp the forecast lacks. A minutes column that falls while BPM holds says the ramp is a minutes restriction; a BPM column that falls says the player plays worse.

A worth figure is per game played. The availability column says how often he played; the worth per team game, which an engine would use, is the two multiplied.

# Limits

- A run of missed games is read as an injury. Suspensions, rest, personal leave and a coach's decision look the same. A five-game run of healthy scratches dilutes the short bucket.
- Players who never came back are not in the report. The curve describes the players who returned.
- The windows are team games, not calendar days. Twenty team games is about six weeks.
- A player who joins a team during the season reads as absent before his first game with it, which blocks the control stretches before that and keeps a run soon after his arrival from being an event.
- A late window cut short by the end of the season weighs as much as a full one in a cell's late means; its availability over a game or two is noise.
- One report is computed at a time; a second request while one runs answers 429. A season is computed once and cached, so a second range over the same seasons costs only the grouping.
- The control samples of one player overlap, so their count overstates the independent evidence; the effect's error uses it as if they did not.
- The report has no cause. For the seasons with stored injury reports, a later version could label the runs.
- The first run of a range computes every season's box-score table that is not cached yet; a long range takes seconds to a minute. The report is cached until the next import or sync.

# Where it shows

The Players page, section **Return from absence**: a range of seasons, a **Run** button, the cells and, on request, the events. `GET /api/research/return-curve` serves it; see the [API reference](../reference/api.md).
