-- Market tracking: each game's model numbers and sportsbook lines as they stood in the last
-- capture before tip-off, kept to score the model against the market once the game is
-- played. Keyed by date and team codes, as the game reports are, so a re-import of the
-- games keeps them. A later capture replaces an earlier one only while the game has not
-- started.
CREATE TABLE market_snapshots (
    game_date          DATE NOT NULL,
    home_code          TEXT NOT NULL,
    away_code          TEXT NOT NULL,
    season             INTEGER NOT NULL,
    captured_at        TIMESTAMPTZ NOT NULL,
    start_time         TIMESTAMPTZ,
    model_version      TEXT NOT NULL,
    app_version        TEXT NOT NULL,
    home_advantage     DOUBLE PRECISION NOT NULL,
    home_rating        DOUBLE PRECISION NOT NULL,
    away_rating        DOUBLE PRECISION NOT NULL,
    home_stored        DOUBLE PRECISION NOT NULL,
    away_stored        DOUBLE PRECISION NOT NULL,
    home_penalty       DOUBLE PRECISION NOT NULL,
    away_penalty       DOUBLE PRECISION NOT NULL,
    p_home_full        DOUBLE PRECISION NOT NULL,
    p_home_model       DOUBLE PRECISION NOT NULL,
    home_margin_model  DOUBLE PRECISION NOT NULL,
    provider           TEXT NOT NULL,
    source             TEXT NOT NULL,
    home_spread        DOUBLE PRECISION,
    home_spread_odds   DOUBLE PRECISION,
    away_spread_odds   DOUBLE PRECISION,
    home_moneyline     DOUBLE PRECISION,
    away_moneyline     DOUBLE PRECISION,
    total              DOUBLE PRECISION,
    p_home_market      DOUBLE PRECISION,
    book_margin        DOUBLE PRECISION,
    PRIMARY KEY (game_date, home_code, away_code)
);
CREATE INDEX market_snapshots_season ON market_snapshots (season, game_date);
