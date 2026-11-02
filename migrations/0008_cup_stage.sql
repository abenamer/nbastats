-- The NBA Cup stage of a game as the results source labels it (East Group A, West
-- Quarterfinal, Championship, ...), from the 2025-26 season on; null for every other game.
-- The Cup dates and groups of a season follow from these labels, so no curated table is
-- needed for a new season.
ALTER TABLE games ADD COLUMN cup_stage TEXT;
