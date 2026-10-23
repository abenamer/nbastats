# Research

Research that informs the application but is not part of the documentation bundle in `docs/`: literature reviews, comparisons with published models, and the notes and scripts behind them. Each study has its own folder with a `report.md`, the `notes/` it was written from and any `scripts/` used for the computations. Papers that a study cites go in `papers/` (see its README).

| Study | Question | Date |
|-------|----------|------|
| [Basketball simulation accuracy gap](basketball-simulation-accuracy-gap/report.md) | What the MIT Sloan Sports Analytics Conference papers and the wider literature say about basketball prediction, and how far a team-Elo Monte Carlo like this application's sits from the betting market, player-level models and the predictability ceiling. | 2026-10-23 |

## How the studies were made

The notes record where each figure came from and mark the author's own computations. The first study was written in a sandbox that could not open the papers themselves, so its external figures come from abstracts, search snippets and public data repositories; the report says so, and the figures are to be checked against the full papers once they are in `papers/`.

The application's own numbers (game-level accuracy, Brier score, log loss and calibration of the pre-game Elo forecast; the championship backtest) are computed from the application's database and are listed in each study's `notes/our_model_numbers.md`.

## Running the scripts

The scripts take their input files as arguments. The two FiveThirtyEight scorers need only the Python standard library; the power simulation needs numpy. The SQL file reproduces the application's own numbers against its database.

    python3 research/basketball-simulation-accuracy-gap/scripts/eval538.py <nba-forecast CSV from fivethirtyeight/checking-our-work-data>
    python3 research/basketball-simulation-accuracy-gap/scripts/eval538b.py <nba_playoffs.csv> <nba_games.csv>
    python3 research/basketball-simulation-accuracy-gap/scripts/power.py
    psql "$DATABASE_URL" -f research/basketball-simulation-accuracy-gap/scripts/our_model_numbers.sql
