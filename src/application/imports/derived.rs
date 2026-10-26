//! Rebuild derived tables (seasons, team_seasons, championships) after an import.

use std::collections::{HashMap, HashSet};

use chrono::{Datelike, NaiveDate};
use tracing::{info, warn};

use crate::application::ports::{
    AnalysisStore, ChampionshipSource, GameStore, NewChampionship, SeasonSummary, StoreError,
};
use crate::domain::ids::{SeasonYear, TeamId};
use crate::domain::models::{
    Conference, EloSource, Game, GameType, PlayoffResult, ResultSource, Team, TeamSeason,
};
use crate::domain::playoffs::reconstruct;
use crate::domain::reference;

/// Classification of the non-playoff games of a season.
#[derive(Debug, Default)]
pub struct NonPlayoffClasses {
    pub play_in: Vec<i64>,
    pub cup_final: Vec<i64>,
    pub regular: Vec<i64>,
}

/// Classify non-playoff games into regular-season, play-in and NBA Cup final games.
pub fn classify_non_playoff(season: SeasonYear, games: &[Game]) -> NonPlayoffClasses {
    let year = season.end_year();
    let mut out = NonPlayoffClasses::default();
    // NBA Cup final (since 2023-24): the only game on a December date; it does not count in
    // the standings and both teams finish with 83 games.
    let mut per_date: HashMap<NaiveDate, usize> = HashMap::new();
    let mut totals: HashMap<TeamId, i32> = HashMap::new();
    for g in games.iter().filter(|g| !g.is_playoff()) {
        *per_date.entry(g.game_date).or_insert(0) += 1;
        if g.is_played() {
            *totals.entry(g.home_team_id).or_insert(0) += 1;
            *totals.entry(g.away_team_id).or_insert(0) += 1;
        }
    }
    // A game the source marked as the Cup final keeps the type; the date rule finds the
    // final of an archive season, where the totals are complete.
    let is_cup_final = |g: &Game| {
        g.game_type == GameType::CupFinal
            || (year >= 2024
                && g.game_date.month() == 12
                && per_date.get(&g.game_date) == Some(&1)
                && totals.get(&g.home_team_id).copied().unwrap_or(0) >= 83
                && totals.get(&g.away_team_id).copied().unwrap_or(0) >= 83)
    };
    if year == 2020 {
        let play_in_day = NaiveDate::from_ymd_opt(2020, 8, 15).expect("valid date");
        for g in games.iter().filter(|g| !g.is_playoff()) {
            if is_cup_final(g) {
                out.cup_final.push(g.id);
            } else if g.game_date == play_in_day {
                out.play_in.push(g.id);
            } else {
                out.regular.push(g.id);
            }
        }
        return out;
    }
    if year < 2021 {
        for g in games.iter().filter(|g| !g.is_playoff()) {
            if is_cup_final(g) {
                out.cup_final.push(g.id);
            } else {
                out.regular.push(g.id);
            }
        }
        return out;
    }
    // Teams not in the play-in play exactly the regular-season length; anything beyond is play-in.
    let mut count: HashMap<TeamId, i32> = HashMap::new();
    for g in games
        .iter()
        .filter(|g| !g.is_playoff() && g.is_played() && !is_cup_final(g))
    {
        *count.entry(g.home_team_id).or_insert(0) += 1;
        *count.entry(g.away_team_id).or_insert(0) += 1;
    }
    let season_len = count.values().copied().min().unwrap_or(0);
    // In-progress seasons: no team has finished, so nothing is a play-in game yet.
    let finished = count.values().filter(|&&c| c >= season_len).count() >= 20 && season_len >= 72;
    let mut index: HashMap<TeamId, i32> = HashMap::new();
    for g in games.iter().filter(|g| !g.is_playoff()) {
        if is_cup_final(g) {
            out.cup_final.push(g.id);
            continue;
        }
        let h_idx = {
            let e = index.entry(g.home_team_id).or_insert(0);
            *e += 1;
            *e
        };
        let a_idx = {
            let e = index.entry(g.away_team_id).or_insert(0);
            *e += 1;
            *e
        };
        if finished && (h_idx > season_len || a_idx > season_len) {
            out.play_in.push(g.id);
        } else {
            out.regular.push(g.id);
        }
    }
    out
}

pub async fn rebuild_seasons<S: GameStore>(
    store: &S,
    seasons: &[SeasonYear],
) -> Result<(), StoreError> {
    let teams = store.team_map().await?;
    let curated: HashMap<SeasonYear, (&str, &str)> = reference::CURATED_CHAMPIONS
        .iter()
        .map(|c| (SeasonYear::of(c.0), (c.1, c.2)))
        .collect();
    for &season in seasons {
        let mut games = store.season_games(season).await?;
        // 1. game classification
        let classes = classify_non_playoff(season, &games);
        let type_of: HashMap<i64, GameType> = games.iter().map(|g| (g.id, g.game_type)).collect();
        let mut changed = false;
        for (ids, wanted) in [
            (classes.play_in, GameType::PlayIn),
            (classes.cup_final, GameType::CupFinal),
            (classes.regular, GameType::Regular),
        ] {
            let ids: Vec<i64> = ids
                .into_iter()
                .filter(|id| type_of.get(id) != Some(&wanted))
                .collect();
            if !ids.is_empty() {
                store.set_game_types(&ids, wanted).await?;
                changed = true;
            }
        }
        if changed {
            games = store.season_games(season).await?;
        }
        // 2. series / results
        let series = reconstruct::series_from_games(season, &games);
        let complete = reconstruct::postseason_complete(&series);
        let results = reconstruct::playoff_results(&series);
        let seeds: HashMap<TeamId, i32> = reconstruct::reconstruct(season, &games, &teams)
            .map(|r| {
                r.bracket
                    .teams
                    .iter()
                    .map(|t| (t.team_id, i32::from(t.seed)))
                    .collect()
            })
            .unwrap_or_default();
        // 3. season summary
        let mut team_ids: Vec<TeamId> = games
            .iter()
            .flat_map(|g| [g.home_team_id, g.away_team_id])
            .collect();
        team_ids.sort_unstable();
        team_ids.dedup();
        store
            .update_season_summary(&SeasonSummary {
                season,
                team_count: team_ids.len() as i32,
                game_count: games.len() as i32,
                computed_game_count: games
                    .iter()
                    .filter(|g| g.elo_source == EloSource::Computed)
                    .count() as i32,
                archive_game_count: games
                    .iter()
                    .filter(|g| g.result_source == ResultSource::Archive)
                    .count() as i32,
                scheduled_game_count: games.iter().filter(|g| !g.is_played()).count() as i32,
                playoff_game_count: games.iter().filter(|g| g.is_playoff()).count() as i32,
                play_in_game_count: games
                    .iter()
                    .filter(|g| g.game_type == GameType::PlayIn)
                    .count() as i32,
                cup_final_game_count: games
                    .iter()
                    .filter(|g| g.game_type == GameType::CupFinal)
                    .count() as i32,
                regular_season_end: games
                    .iter()
                    .filter(|g| g.game_type == GameType::Regular && g.is_played())
                    .map(|g| g.game_date)
                    .max(),
                playoffs_start: games
                    .iter()
                    .filter(|g| g.is_playoff())
                    .map(|g| g.game_date)
                    .min(),
                playoffs_end: games
                    .iter()
                    .filter(|g| g.is_playoff())
                    .map(|g| g.game_date)
                    .max(),
                complete,
            })
            .await?;
        // 4. team seasons
        let alignment = reconstruct::alignment_map(season, &teams, &games);
        let play_in_teams: HashSet<TeamId> = games
            .iter()
            .filter(|g| g.game_type == GameType::PlayIn)
            .flat_map(|g| [g.home_team_id, g.away_team_id])
            .collect();
        let playoff_teams: HashSet<TeamId> = games
            .iter()
            .filter(|g| g.is_playoff())
            .flat_map(|g| [g.home_team_id, g.away_team_id])
            .collect();
        for &tid in &team_ids {
            let Some(team) = teams.get(&tid) else {
                continue;
            };
            let ctx = TeamSeasonInputs {
                season,
                games: &games,
                alignment: &alignment,
                results: &results,
                seeds: &seeds,
                play_in_teams: &play_in_teams,
                playoff_teams: &playoff_teams,
            };
            store.upsert_team_season(&team_season(team, &ctx)).await?;
        }
        // 5. championship
        if let Some((winner, loser)) = reconstruct::champion(&series) {
            let fran = teams[&winner].franchise_id;
            store
                .upsert_championship(&NewChampionship {
                    season,
                    champion: winner,
                    champion_franchise: fran,
                    runner_up: Some(loser),
                    source: ChampionshipSource::Derived,
                })
                .await?;
            if let Some((ck, _)) = curated.get(&season) {
                let wk = teams[&winner].franchise_key();
                if wk != *ck && teams[&winner].code != *ck {
                    warn!(%season, derived = wk, curated = ck, "derived champion disagrees with curated record");
                }
            }
        } else if let Some((ck, rk)) = curated.get(&season) {
            // No complete playoff data: fall back to the curated record (never invent Elo).
            let champ = teams.values().find(|t| &t.code == ck);
            let runner_up = teams.values().find(|t| &t.code == rk);
            if let Some(c) = champ {
                store
                    .upsert_championship(&NewChampionship {
                        season,
                        champion: c.id,
                        champion_franchise: c.franchise_id,
                        runner_up: runner_up.map(|t| t.id),
                        source: ChampionshipSource::Curated,
                    })
                    .await?;
            }
        }
        info!(%season, games = games.len(), complete, "derived data rebuilt");
    }
    Ok(())
}

struct TeamSeasonInputs<'a> {
    season: SeasonYear,
    games: &'a [Game],
    alignment: &'a HashMap<TeamId, (Conference, String)>,
    results: &'a HashMap<TeamId, PlayoffResult>,
    seeds: &'a HashMap<TeamId, i32>,
    play_in_teams: &'a HashSet<TeamId>,
    playoff_teams: &'a HashSet<TeamId>,
}

fn team_season(team: &Team, ctx: &TeamSeasonInputs<'_>) -> TeamSeason {
    let tid = team.id;
    let (mut wins, mut losses) = (0, 0);
    let mut elo_start = None;
    let mut elo_regular_end = None;
    let mut elo_pre_playoffs = None;
    let mut elo_end = None;
    let mut elo_peak: Option<f64> = None;
    for g in ctx.games.iter().filter(|g| g.involves(tid)) {
        let is_home = g.home_team_id == tid;
        let (pre, post) = g.elo_of(tid);
        elo_start.get_or_insert(pre);
        let Some(outcome) = g.outcome() else { continue };
        elo_end = Some(post);
        elo_peak = Some(elo_peak.map_or(post, |p: f64| p.max(post)));
        match g.game_type {
            GameType::Regular => {
                elo_regular_end = Some(post);
                elo_pre_playoffs = Some(post);
                if outcome.home_won() == is_home {
                    wins += 1;
                } else {
                    losses += 1;
                }
            }
            GameType::PlayIn | GameType::CupFinal => elo_pre_playoffs = Some(post),
            GameType::Playoff => {}
        }
    }
    let (conference, division) = ctx
        .alignment
        .get(&tid)
        .map_or((None, None), |(c, d)| (Some(*c), Some(d.clone())));
    let made_playoffs = ctx.playoff_teams.contains(&tid);
    let made_play_in = ctx.play_in_teams.contains(&tid);
    let playoff_result = ctx.results.get(&tid).copied().unwrap_or(if made_play_in {
        PlayoffResult::PlayIn
    } else {
        PlayoffResult::Missed
    });
    TeamSeason {
        season: ctx.season,
        team_id: tid,
        franchise_id: team.franchise_id,
        conference,
        division,
        wins,
        losses,
        win_pct: if wins + losses > 0 {
            f64::from(wins) / f64::from(wins + losses)
        } else {
            0.0
        },
        elo_start,
        elo_regular_end,
        elo_pre_playoffs,
        elo_end,
        elo_peak,
        made_play_in,
        made_playoffs,
        seed: ctx.seeds.get(&tid).copied(),
        playoff_result,
    }
}

/// Seasons in `present` that have a curated champion but no championship row yet.
pub fn missing_curated(
    present: &[SeasonYear],
    have: &HashSet<SeasonYear>,
) -> Vec<(SeasonYear, &'static str, &'static str)> {
    reference::CURATED_CHAMPIONS
        .iter()
        .map(|c| (SeasonYear::of(c.0), c.1, c.2))
        .filter(|(s, _, _)| present.contains(s) && !have.contains(s))
        .collect()
}

/// Insert curated championships for stored seasons that have none (for installations whose
/// derived data predates a curated record). Derived rows are never replaced.
pub async fn backfill_curated_championships<S: GameStore + AnalysisStore>(
    store: &S,
) -> Result<usize, StoreError> {
    let present: Vec<SeasonYear> = store
        .seasons()
        .await?
        .into_iter()
        .map(|s| s.season)
        .collect();
    let have: HashSet<SeasonYear> = store
        .championships(SeasonYear::MIN, SeasonYear::MAX)
        .await?
        .into_iter()
        .map(|c| c.season)
        .collect();
    let teams = store.teams().await?;
    let mut inserted = 0;
    for (season, ck, rk) in missing_curated(&present, &have) {
        let Some(champ) = teams.iter().find(|t| t.code == ck) else {
            warn!(%season, code = ck, "curated champion code unknown; skipped");
            continue;
        };
        let runner_up = teams.iter().find(|t| t.code == rk);
        store
            .upsert_championship(&NewChampionship {
                season,
                champion: champ.id,
                champion_franchise: champ.franchise_id,
                runner_up: runner_up.map(|t| t.id),
                source: ChampionshipSource::Curated,
            })
            .await?;
        inserted += 1;
    }
    if inserted > 0 {
        info!(inserted, "curated championships backfilled");
    }
    Ok(inserted)
}

#[cfg(test)]
mod curated_tests {
    use super::*;

    #[test]
    fn missing_curated_fills_only_stored_seasons_without_a_row() {
        let present = vec![
            SeasonYear::of(1947),
            SeasonYear::of(1948),
            SeasonYear::of(2016),
        ];
        let have: HashSet<SeasonYear> = [SeasonYear::of(1948), SeasonYear::of(2016)]
            .into_iter()
            .collect();
        let missing = missing_curated(&present, &have);
        assert_eq!(missing, vec![(SeasonYear::of(1947), "PHW", "CHS")]);
    }
}
