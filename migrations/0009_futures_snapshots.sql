-- Title futures: each team's chance to win the title as the sportsbook priced it and as the
-- model forecast it on one day, saved once a day while the regular season runs, to score
-- the two against the champion once the season is decided. One row per team per day; a
-- later capture of the same day replaces an earlier one.
CREATE TABLE futures_snapshots (
    season          INTEGER NOT NULL,
    snapshot_date   DATE NOT NULL,
    team_code       TEXT NOT NULL,
    captured_at     TIMESTAMPTZ NOT NULL,
    provider        TEXT NOT NULL,
    source          TEXT NOT NULL,
    price           DOUBLE PRECISION NOT NULL,
    p_market        DOUBLE PRECISION NOT NULL,
    book_margin     DOUBLE PRECISION NOT NULL,
    p_model         DOUBLE PRECISION NOT NULL,
    p_model_se      DOUBLE PRECISION NOT NULL,
    model_version   TEXT NOT NULL,
    app_version     TEXT NOT NULL,
    PRIMARY KEY (season, snapshot_date, team_code)
);
