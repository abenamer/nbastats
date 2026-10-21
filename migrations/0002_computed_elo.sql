-- Elo provenance: 'imported' rows carry ratings from the source dataset; 'computed' rows carry
-- ratings the application calculated from game results with the FiveThirtyEight formulas
-- (seasons after the dataset ends, or a projected season).
ALTER TABLE games ADD COLUMN elo_source TEXT NOT NULL DEFAULT 'imported'
    CHECK (elo_source IN ('imported', 'computed'));
CREATE INDEX games_elo_source_idx ON games(season, elo_source);
ALTER TABLE seasons ADD COLUMN computed_game_count INTEGER NOT NULL DEFAULT 0;
ALTER TABLE seasons ADD COLUMN scheduled_game_count INTEGER NOT NULL DEFAULT 0;
