-- Players, box scores and injury reports from the player source.
--
-- A player is one row per source identifier, so a player stays one row across teams and
-- seasons. An appearance is one player's line in the box score of a stored game; a line
-- whose game or team the store does not have is counted, not stored. Each date fetched is
-- recorded so the backfill can resume, and each day's injury report is kept as history.
CREATE TABLE players (
    id            SERIAL PRIMARY KEY,
    source        TEXT NOT NULL,
    source_id     BIGINT NOT NULL,
    first_name    TEXT NOT NULL,
    last_name     TEXT NOT NULL,
    position      TEXT,
    height        TEXT,
    weight        TEXT,
    college       TEXT,
    country       TEXT,
    draft_year    INTEGER,
    draft_round   INTEGER,
    draft_number  INTEGER,
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (source, source_id)
);

CREATE TABLE appearances (
    game_id    BIGINT NOT NULL REFERENCES games(id) ON DELETE CASCADE,
    player_id  INTEGER NOT NULL REFERENCES players(id),
    team_id    INTEGER NOT NULL REFERENCES teams(id),
    seconds    INTEGER,
    pts        INTEGER,
    reb        INTEGER,
    oreb       INTEGER,
    dreb       INTEGER,
    ast        INTEGER,
    stl        INTEGER,
    blk        INTEGER,
    tov        INTEGER,
    pf         INTEGER,
    fgm        INTEGER,
    fga        INTEGER,
    fg3m       INTEGER,
    fg3a       INTEGER,
    ftm        INTEGER,
    fta        INTEGER,
    PRIMARY KEY (game_id, player_id)
);
CREATE INDEX appearances_player_idx ON appearances (player_id, game_id);
CREATE INDEX appearances_team_idx ON appearances (team_id, game_id);

CREATE TABLE box_score_fetches (
    game_date   DATE PRIMARY KEY,
    source      TEXT NOT NULL,
    fetched_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    lines       INTEGER NOT NULL,
    matched     INTEGER NOT NULL,
    unmatched   INTEGER NOT NULL
);

CREATE TABLE injury_reports (
    report_date  DATE NOT NULL,
    player_id    INTEGER NOT NULL REFERENCES players(id),
    team_id      INTEGER NOT NULL REFERENCES teams(id),
    status       TEXT NOT NULL,
    return_date  DATE,
    description  TEXT,
    fetched_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (report_date, player_id)
);
CREATE INDEX injury_reports_player_idx ON injury_reports (player_id, report_date);
