-- Compute first: the application rates every game itself. The archive's ratings stay beside
-- the application's as reference values, and each game records where its result came from.
ALTER TABLE games ADD COLUMN result_source TEXT NOT NULL DEFAULT 'archive'
    CHECK (result_source IN ('archive', 'results', 'projection'));
ALTER TABLE games ADD COLUMN ref_home_elo_pre DOUBLE PRECISION;
ALTER TABLE games ADD COLUMN ref_away_elo_pre DOUBLE PRECISION;
ALTER TABLE games ADD COLUMN ref_home_elo_post DOUBLE PRECISION;
ALTER TABLE games ADD COLUMN ref_away_elo_post DOUBLE PRECISION;
-- Existing rows: the result source follows the import that stored them; imported ratings
-- become the reference values. The application recomputes the ratings at its next start.
UPDATE games g SET result_source = CASE d.format WHEN 'results' THEN 'results' WHEN 'projection' THEN 'projection' ELSE 'archive' END
    FROM data_imports d WHERE d.id = g.import_id;
UPDATE games SET ref_home_elo_pre = home_elo_pre, ref_away_elo_pre = away_elo_pre,
                 ref_home_elo_post = home_elo_post, ref_away_elo_post = away_elo_post
    WHERE elo_source = 'imported';
CREATE INDEX games_result_source_idx ON games(season, result_source);
ALTER TABLE seasons ADD COLUMN archive_game_count INTEGER NOT NULL DEFAULT 0;
