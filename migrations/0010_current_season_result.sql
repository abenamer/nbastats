-- A season whose playoffs have not started gives every team outside the play-in the playoff
-- result 'current' instead of 'missed': nothing is decided yet, so a season in progress must
-- not read as a season of misses. The constraint admits the value, and the stored rows of such
-- seasons take it, as the derived data sets it on every rebuild.
ALTER TABLE team_seasons DROP CONSTRAINT team_seasons_playoff_result_check;
ALTER TABLE team_seasons ADD CONSTRAINT team_seasons_playoff_result_check
    CHECK (playoff_result IN ('current', 'missed', 'play_in', 'first_round', 'second_round', 'conf_finals', 'finals', 'champion'));
UPDATE team_seasons ts SET playoff_result = 'current'
WHERE ts.playoff_result = 'missed'
  AND NOT EXISTS (
      SELECT 1 FROM games g
      WHERE g.season = ts.season AND g.game_type = 'playoff' AND g.home_score IS NOT NULL
  );
