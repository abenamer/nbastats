-- NBA Elo Championship Simulator: initial schema
-- All tables live in the default schema. Timestamps are stored in UTC.

CREATE TABLE franchises (
    id            SERIAL PRIMARY KEY,
    key           TEXT NOT NULL UNIQUE,          -- stable franchise identifier, e.g. 'GSW'
    name          TEXT NOT NULL,                 -- current franchise name
    active        BOOLEAN NOT NULL DEFAULT TRUE,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- A team is a (code, era) identity used by the source data (e.g. SEA, OKC are two
-- teams of the same franchise). Codes follow Basketball-Reference / FiveThirtyEight.
CREATE TABLE teams (
    id            SERIAL PRIMARY KEY,
    code          TEXT NOT NULL UNIQUE,
    franchise_id  INTEGER NOT NULL REFERENCES franchises(id),
    name          TEXT NOT NULL,
    first_season  INTEGER,
    last_season   INTEGER,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX teams_franchise_idx ON teams(franchise_id);

CREATE TABLE seasons (
    season        INTEGER PRIMARY KEY,           -- season end year: 2004 == 2003-04
    label         TEXT NOT NULL,
    team_count    INTEGER NOT NULL DEFAULT 0,
    game_count    INTEGER NOT NULL DEFAULT 0,
    playoff_game_count INTEGER NOT NULL DEFAULT 0,
    play_in_game_count INTEGER NOT NULL DEFAULT 0,
    cup_final_game_count INTEGER NOT NULL DEFAULT 0,
    regular_season_end DATE,
    playoffs_start DATE,
    playoffs_end  DATE,
    complete      BOOLEAN NOT NULL DEFAULT FALSE, -- playoffs present and finished
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Conference / division membership per season (reference data, synced at startup)
CREATE TABLE alignments (
    season        INTEGER NOT NULL,
    team_id       INTEGER NOT NULL REFERENCES teams(id),
    conference    TEXT NOT NULL CHECK (conference IN ('East', 'West')),
    division      TEXT NOT NULL,
    PRIMARY KEY (season, team_id)
);

CREATE TABLE data_imports (
    id            BIGSERIAL PRIMARY KEY,
    source        TEXT NOT NULL,                 -- 'bundled', 'upload', 'download'
    source_url    TEXT,
    file_name     TEXT,
    file_sha256   TEXT,
    format        TEXT NOT NULL,                 -- 'fivethirtyeight_nba_elo' | 'fivethirtyeight_nbaallelo'
    status        TEXT NOT NULL CHECK (status IN ('running', 'completed', 'failed')),
    rows_read     BIGINT NOT NULL DEFAULT 0,
    games_inserted BIGINT NOT NULL DEFAULT 0,
    games_updated BIGINT NOT NULL DEFAULT 0,
    games_unchanged BIGINT NOT NULL DEFAULT 0,
    rows_rejected BIGINT NOT NULL DEFAULT 0,
    min_season    INTEGER,
    max_season    INTEGER,
    error         TEXT,
    notes         TEXT,
    started_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    finished_at   TIMESTAMPTZ
);

CREATE TABLE games (
    id              BIGSERIAL PRIMARY KEY,
    season          INTEGER NOT NULL REFERENCES seasons(season),
    game_date       DATE NOT NULL,
    seq             SMALLINT NOT NULL DEFAULT 0,  -- disambiguates same-day double-headers
    home_team_id    INTEGER NOT NULL REFERENCES teams(id),
    away_team_id    INTEGER NOT NULL REFERENCES teams(id),
    neutral         BOOLEAN NOT NULL DEFAULT FALSE,
    game_type       TEXT NOT NULL CHECK (game_type IN ('regular', 'play_in', 'cup_final', 'playoff')),
    home_score      INTEGER,
    away_score      INTEGER,
    home_elo_pre    DOUBLE PRECISION NOT NULL,
    away_elo_pre    DOUBLE PRECISION NOT NULL,
    home_elo_post   DOUBLE PRECISION NOT NULL,
    away_elo_post   DOUBLE PRECISION NOT NULL,
    home_win_prob   DOUBLE PRECISION,             -- source forecast for the home team
    import_id       BIGINT REFERENCES data_imports(id),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (home_team_id <> away_team_id),
    UNIQUE (game_date, home_team_id, away_team_id, seq)
);
CREATE INDEX games_season_date_idx ON games(season, game_date, id);
CREATE INDEX games_home_idx ON games(home_team_id, game_date);
CREATE INDEX games_away_idx ON games(away_team_id, game_date);
CREATE INDEX games_type_idx ON games(season, game_type);

-- Derived per-team season summary, rebuilt after each import.
CREATE TABLE team_seasons (
    season            INTEGER NOT NULL REFERENCES seasons(season),
    team_id           INTEGER NOT NULL REFERENCES teams(id),
    franchise_id      INTEGER NOT NULL REFERENCES franchises(id),
    conference        TEXT,
    division          TEXT,
    wins              INTEGER NOT NULL DEFAULT 0,
    losses            INTEGER NOT NULL DEFAULT 0,
    win_pct           DOUBLE PRECISION NOT NULL DEFAULT 0,
    elo_start         DOUBLE PRECISION,           -- Elo entering the first game of the season
    elo_regular_end   DOUBLE PRECISION,           -- Elo after the last regular-season game
    elo_pre_playoffs  DOUBLE PRECISION,           -- Elo after the last game before the playoffs (incl. play-in)
    elo_end           DOUBLE PRECISION,           -- Elo after the last game of the season
    elo_peak          DOUBLE PRECISION,
    made_play_in      BOOLEAN NOT NULL DEFAULT FALSE,
    made_playoffs     BOOLEAN NOT NULL DEFAULT FALSE,
    seed              INTEGER,
    playoff_result    TEXT NOT NULL DEFAULT 'missed'
        CHECK (playoff_result IN ('missed', 'play_in', 'first_round', 'second_round', 'conf_finals', 'finals', 'champion')),
    PRIMARY KEY (season, team_id)
);
CREATE INDEX team_seasons_franchise_idx ON team_seasons(franchise_id, season);

CREATE TABLE championships (
    season              INTEGER PRIMARY KEY REFERENCES seasons(season),
    champion_team_id    INTEGER NOT NULL REFERENCES teams(id),
    champion_franchise_id INTEGER NOT NULL REFERENCES franchises(id),
    runner_up_team_id   INTEGER REFERENCES teams(id),
    source              TEXT NOT NULL,            -- 'derived' (from game records) or 'curated'
    updated_at          TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Simulation jobs: configuration, progress, results and versioning. Named jobs are "scenarios".
CREATE TABLE simulation_jobs (
    id              UUID PRIMARY KEY,
    kind            TEXT NOT NULL CHECK (kind IN ('alpha', 'playoff', 'season', 'backtest', 'dynasty')),
    name            TEXT,
    status          TEXT NOT NULL CHECK (status IN ('queued', 'running', 'completed', 'failed', 'interrupted', 'cancelled')),
    params          JSONB NOT NULL,
    result          JSONB,
    progress        DOUBLE PRECISION NOT NULL DEFAULT 0,
    error           TEXT,
    seed            BIGINT,
    app_version     TEXT NOT NULL,
    model_version   TEXT NOT NULL,
    attempts        INTEGER NOT NULL DEFAULT 0,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    started_at      TIMESTAMPTZ,
    finished_at     TIMESTAMPTZ,
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX simulation_jobs_status_idx ON simulation_jobs(status, created_at);
CREATE INDEX simulation_jobs_kind_idx ON simulation_jobs(kind, created_at DESC);
CREATE INDEX simulation_jobs_params_idx ON simulation_jobs USING GIN (params);

-- Per-team pre-playoff forecasts produced by backtest runs (one row per model/season/team).
CREATE TABLE forecasts (
    id              BIGSERIAL PRIMARY KEY,
    job_id          UUID NOT NULL REFERENCES simulation_jobs(id) ON DELETE CASCADE,
    model           TEXT NOT NULL,
    season          INTEGER NOT NULL REFERENCES seasons(season),
    team_id         INTEGER NOT NULL REFERENCES teams(id),
    seed            INTEGER,
    p_second_round  DOUBLE PRECISION NOT NULL,
    p_conf_finals   DOUBLE PRECISION NOT NULL,
    p_finals        DOUBLE PRECISION NOT NULL,
    p_champion      DOUBLE PRECISION NOT NULL,
    actual_result   TEXT NOT NULL,
    actual_champion BOOLEAN NOT NULL,
    information_cutoff DATE NOT NULL,             -- last game date whose information was used
    UNIQUE (job_id, model, season, team_id)
);
CREATE INDEX forecasts_season_idx ON forecasts(season, model);
