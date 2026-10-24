---
type: Concept
title: Rule periods and bargaining periods
description: The two period tables that group seasons on the Records page, rule changes and collective bargaining agreements, with what each period changed.
tags: [records, eras, cba, rules]
generated: { by: "human:abenamer", at: "2026-10-23T11:00:00Z" }
sources:
  - id: records
    resource: /src/domain/records.rs
    title: Rule periods
  - id: cba
    resource: /src/domain/cba.rs
    title: Bargaining periods
---

# Purpose

The Records page groups seasons by period in two ways. A rule period starts with a change in how the game is played; a bargaining period starts with the season in which an agreement between the league and the players' association took effect. Both are tables in the domain code, curated from the public record, and each period carries a summary and the changes it brought. The text in this page follows the tables; the tables are the source.

Read a difference between periods as association, not cause: a period boundary also coincides with expansion, with other rule changes and with the growth of three-point shooting.

# Rule periods

| League | Period | From | Rule change at the start |
|--------|--------|------|--------------------------|
| NBA/BAA | Pre shot clock | 1946-47 | The BAA's founding; no shot clock |
| NBA/BAA | Shot clock | 1954-55 | The 24-second shot clock |
| NBA/BAA | Three-point line | 1979-80 | The 3-point line |
| NBA/BAA | Zone defense | 2001-02 | Illegal-defense rules abolished; zone defense allowed |
| NBA/BAA | Hand-check ban | 2004-05 | Perimeter hand-checking enforced |
| ABA | ABA | 1967-68 | The 3-point line and a 30-second clock from the first season |

An era is a run of seasons with the same number of teams inside one rule period; see the [application overview](application.md).

# Bargaining periods

The ABA had no agreement of this kind, so only NBA/BAA seasons belong to a bargaining period. Dates are the first season an agreement applied to.

| Period | From | What it brought |
|--------|------|-----------------|
| Pre-cap | 1946-47 | Reserve clause, no salary cap; pay set by each negotiation, pushed up by the ABA from 1967-68. The players' association forms in 1954-55 and wins a pension plan in 1963-64; the Robertson suit is filed in 1970-71. |
| Robertson settlement | 1976-77 | The reserve clause ends; free agency with compensation, then the right of first refusal from 1981-82. The ABA merger adds four teams. The 1983 agreement sets the first cap for 1984-85. |
| First salary cap | 1984-85 | A cap of about 3.6 million per team (53 percent of revenue), the Bird exception, a minimum payroll and an anti-drug agreement. |
| 1988 agreement | 1988-89 | Unrestricted free agency phased in, the draft cut to two rounds, expansion to 27 teams; rookie pay still unregulated. |
| 1995 agreement | 1995-96 | Rookie wage scale, a minimum team salary, Toronto and Vancouver; no maximum contract, which leads to the 1998 lockout. |
| 1999 agreement | 1998-99 | After the lockout (50 games): maximum salaries by years of service, a luxury tax, escrow, the mid-level exception, contracts capped at seven years. |
| 2005 agreement | 2005-06 | Draft age 19, shorter contracts and smaller raises, a yearly dollar-for-dollar tax, a one-time amnesty, 57 percent of revenue to the players. |
| 2011 agreement | 2011-12 | After the lockout (66 games): a revenue split near 50-50, a progressive luxury tax with a repeater rate, the tax apron, shorter contracts. |
| 2017 agreement | 2017-18 | Supermax extensions, two-way contracts, a longer calendar with fewer back-to-backs; pandemic amendments and the permanent play-in. |
| 2023 agreement | 2023-24 | A second apron with hard limits on trades and exceptions, a 65-game rule for awards, the NBA Cup, cap growth limited to 10 percent a year. |

# Competition formats

The Records page also offers the competition format, curated in `domain::eras`: early playoffs from 1946-47 (division-based fields that changed often), the 16-team field from 1983-84, best-of-seven in every round from 2002-03, the play-in era from 2020-21 and the NBA Cup era from 2023-24. ABA seasons are one format of their own. The rule periods above also carry dated changes, shown in the notes of the [era builder](era-builder.md).

# Themed eras

Two themed eras, curated in `domain::eras`, name how the game was played or followed rather than a rule or an agreement: the Reddit NBA era from 2008-09 (the seasons followed on r/nba, with a game thread and a post-game thread for every game) and the three-point dominance era from 2015-16 (the season of Stephen Curry's 402 threes and the Warriors' 73 wins, after which the league's three-point attempts rise every season). Themes overlap, so a season belongs to every theme active in it, and neither has ended.

# The records of a bargaining period

For each period present in the data, the Records page shows: the seasons it covers, the best team-season (season peak) and the weakest (season low), the titles and the franchises that won them (a relocated champion keeps its count), the number of distinct champions, the title HHI, the mean Elo, the Elo spread and the champion z.

* **Title HHI** is the Herfindahl-Hirschman index of the titles: the sum of each champion's squared share of the period's titles. It is 1.0 when one franchise won every title and 1 divided by the number of champions when every champion won once; lower means the titles were spread out.
* **Mean Elo** is the mean season-end rating across the league in each season, averaged over the period's seasons, so every season weighs the same whatever its team count. It answers whether the ratings drift: a game moves points from the loser to the winner and nothing else, so the league's mean changes only through the season-start regression towards the base rating and the teams that join or leave (expansion teams enter at a low rating, teams that fold take their rating with them, the merger teams carried their ABA rating into the NBA). In the archive the NBA/BAA mean by decade is about 1400 in the 1940s (every founding team started at 1300 and a quarter of the gap closes per season), about 1515 in the 1950s (the weakest teams folded in 1950), about 1480 to 1500 in the 1960s and 1970s (with expansion seasons as low as 1440) and within five points of 1505 from the 1980s on, while the highest peaks rise from the low 1700s to the 1860s: the level holds and the tails widen as the league grows.
* **Elo spread** is the standard deviation of season-end ratings across the league in each season, averaged over the period's seasons. A wider spread means a more unequal league.
* **Champion z** is the champion's pre-playoff rating above the league mean in standard deviations of that season's ratings, averaged over the period's seasons with a champion; see the [parity profile](parity.md). A high value means the titles went to teams far above their league.

# Verification

The period tables (rule, bargaining, format, theme) and their changes, and the player careers of the [era builder](era-builder.md), are curated from memory of the public record and need a check against a reference before they are relied on. The checks of this bundle do not verify them.
