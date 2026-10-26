-- Rosters and plus/minus from the player source.
--
-- The whole player database of the source (every player it lists, active or not) and the
-- current rosters (the active players with their team) come from two endpoints beyond the
-- box scores. A player's jersey number and current team are source facts; `active` is
-- whether the last roster refresh listed the player. A box-score line gains the player's
-- plus/minus.
ALTER TABLE players
    ADD COLUMN jersey_number TEXT,
    ADD COLUMN team_id INTEGER REFERENCES teams(id),
    ADD COLUMN active BOOLEAN NOT NULL DEFAULT false;
CREATE INDEX players_team_idx ON players (team_id) WHERE team_id IS NOT NULL;
ALTER TABLE appearances ADD COLUMN plus_minus INTEGER;
