-- Game reports: the link to a wire-service game report (the AP recap that ESPN hosts) for a
-- played game, found by date and team codes so it survives a re-import, and one row per
-- date looked up so a date without reports is not asked for again and again.
CREATE TABLE game_reports (
    game_date  DATE NOT NULL,
    home_code  TEXT NOT NULL,
    away_code  TEXT NOT NULL,
    url        TEXT NOT NULL,
    title      TEXT,
    source     TEXT NOT NULL,
    fetched_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (game_date, home_code, away_code)
);
CREATE TABLE report_lookups (
    game_date  DATE PRIMARY KEY,
    source     TEXT NOT NULL,
    fetched_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    found      INTEGER NOT NULL
);
