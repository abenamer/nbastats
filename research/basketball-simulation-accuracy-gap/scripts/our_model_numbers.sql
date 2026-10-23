-- The application's own game-level scores, as listed in notes/our_model_numbers.md.
-- Run against the application's database: psql "$DATABASE_URL" -f research/basketball-simulation-accuracy-gap/scripts/our_model_numbers.sql
-- Win probability per game: 1 / (1 + 10^(-(home_elo_pre + home bonus - away_elo_pre) / 400)), bonus 100 (0 on a neutral site). Ties excluded.

-- 1. Accuracy, home win rate, Brier and log loss by span and game type
WITH g AS (
  SELECT season, game_type, home_score > away_score AS home_won,
         1.0 / (1.0 + power(10.0, -((home_elo_pre + CASE WHEN neutral THEN 0 ELSE 100 END) - away_elo_pre) / 400.0)) AS p_home
  FROM games WHERE home_score IS NOT NULL AND home_score <> away_score
)
SELECT CASE WHEN season < 2004 THEN '1947-2003' ELSE '2004-2025' END AS span, game_type, count(*) AS games,
       round(avg(CASE WHEN (p_home >= 0.5) = home_won THEN 1 ELSE 0 END)::numeric, 4) AS accuracy,
       round(avg(CASE WHEN home_won THEN 1 ELSE 0 END)::numeric, 4) AS home_win_rate,
       round(avg(power(p_home - CASE WHEN home_won THEN 1 ELSE 0 END, 2))::numeric, 4) AS brier,
       round(avg(-ln(CASE WHEN home_won THEN p_home ELSE 1 - p_home END))::numeric, 4) AS log_loss
FROM g GROUP BY 1, 2 ORDER BY 1, 2;

-- 2. Per season, regular season since 2003-04
WITH g AS (
  SELECT season, home_score > away_score AS home_won,
         1.0 / (1.0 + power(10.0, -((home_elo_pre + CASE WHEN neutral THEN 0 ELSE 100 END) - away_elo_pre) / 400.0)) AS p_home
  FROM games WHERE home_score IS NOT NULL AND home_score <> away_score AND season >= 2004 AND game_type = 'regular'
)
SELECT season, count(*) AS games,
       round(avg(CASE WHEN (p_home >= 0.5) = home_won THEN 1 ELSE 0 END)::numeric, 3) AS accuracy,
       round(avg(power(p_home - CASE WHEN home_won THEN 1 ELSE 0 END, 2))::numeric, 4) AS brier,
       round(avg(-ln(CASE WHEN home_won THEN p_home ELSE 1 - p_home END))::numeric, 4) AS log_loss
FROM g GROUP BY 1 ORDER BY 1;

-- 3. Calibration of the home win probability, regular season since 2003-04
WITH g AS (
  SELECT home_score > away_score AS home_won,
         1.0 / (1.0 + power(10.0, -((home_elo_pre + CASE WHEN neutral THEN 0 ELSE 100 END) - away_elo_pre) / 400.0)) AS p_home
  FROM games WHERE home_score IS NOT NULL AND home_score <> away_score AND season >= 2004 AND game_type = 'regular'
)
SELECT floor(p_home * 10) / 10 AS bin, count(*) AS games, round(avg(p_home)::numeric, 3) AS mean_p,
       round(avg(CASE WHEN home_won THEN 1 ELSE 0 END)::numeric, 3) AS observed
FROM g GROUP BY 1 ORDER BY 1;

-- 4. The championship backtest's per-season spread (the ± values in the docs are standard errors)
WITH latest AS (SELECT job_id FROM forecasts WHERE model = 'elo' ORDER BY information_cutoff DESC, job_id LIMIT 1),
per AS (
  SELECT f.season, sum(power(f.p_champion - CASE WHEN f.actual_champion THEN 1 ELSE 0 END, 2)) AS brier,
         -ln(max(CASE WHEN f.actual_champion THEN f.p_champion END)) AS logloss
  FROM forecasts f JOIN latest l ON l.job_id = f.job_id WHERE f.model = 'elo' GROUP BY f.season)
SELECT count(*) AS seasons, round(avg(brier)::numeric, 3) AS mean_brier, round(stddev_samp(brier)::numeric, 3) AS sd_brier,
       round((stddev_samp(brier) / sqrt(count(*)))::numeric, 3) AS se_brier,
       round(avg(logloss)::numeric, 2) AS mean_log_loss, round(stddev_samp(logloss)::numeric, 2) AS sd_log_loss,
       round((stddev_samp(logloss) / sqrt(count(*)))::numeric, 2) AS se_log_loss
FROM per;
