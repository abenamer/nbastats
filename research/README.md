# Research

Research that informs the application but is not part of the documentation bundle in `docs/`: literature reviews with their notes and scripts, comparisons with published models, and links to the papers they cite (the papers themselves are not stored here). Each study has its own folder with a `report.md`, the `notes/` it was written from and any `scripts/` used for the computations. `papers/README.md` lists every paper the studies cite with a link to where it can be read; a local copy dropped into `papers/` stays untracked.

| Study | Question | Date |
|-------|----------|------|
| [Basketball simulation accuracy gap](basketball-simulation-accuracy-gap/report.md) | What the MIT Sloan Sports Analytics Conference papers and the wider literature say about basketball prediction, and how far a team-Elo Monte Carlo like this application's sits from the betting market, player-level models and the predictability ceiling. | 2026-10-23 |

## How the studies were made

The notes record where each figure came from and mark the author's own computations. The first study was written in a sandbox that could not open the papers themselves, so its external figures come from abstracts, search snippets and public data repositories; the report says so. The study's figures were checked against the full papers on 2026-10-23 (read from local copies), with the per-paper verdicts in `basketball-simulation-accuracy-gap/notes/verification/` (one file per group of papers: Sloan-track, benchmark, model and series papers) and a summary table in the report; figures from web pages and repositories outside the papers listed in `papers/README.md` remain unverified.

The application's own numbers (game-level accuracy, Brier score, log loss and calibration of the pre-game Elo forecast; the championship backtest) are computed from the application's database and are listed in each study's `notes/our_model_numbers.md`.

## Running the scripts

The scripts take their input files as arguments. The two FiveThirtyEight scorers need only the Python standard library; the power simulation needs numpy. The SQL file reproduces the application's own numbers against its database.

    python3 research/basketball-simulation-accuracy-gap/scripts/eval538.py <nba-forecast CSV from fivethirtyeight/checking-our-work-data>
    python3 research/basketball-simulation-accuracy-gap/scripts/eval538b.py <nba_playoffs.csv> <nba_games.csv>
    python3 research/basketball-simulation-accuracy-gap/scripts/power.py
    psql "$DATABASE_URL" -f research/basketball-simulation-accuracy-gap/scripts/our_model_numbers.sql
