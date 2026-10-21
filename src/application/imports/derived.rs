//! Rebuild derived tables (seasons, team_seasons, championships) after an import.

use std::collections::HashMap;

use chrono::NaiveDate;
use tracing::{info, warn};

use crate::adapters::postgres::{self as db, Pool};
use crate::domain::models::{Conference, Game, GameType, PlayoffResult, Team, TeamSeason};
use crate::domain::playoffs::reconstruct;
use crate::domain::reference;

/// Classify non-playoff games into regular-season, play-in and NBA Cup final games.
/// Returns (play_in ids, cup_final ids, regular ids).
pub fn classify_non_playoff(season: i32, games: &[Game]) -> (Vec<i64>, Vec<i64>, Vec<i64>) {
    let mut play_in = Vec::new();
    let mut cup_final = Vec::new();
    let mut regular = Vec::new();
    // NBA Cup final (since 2023-24): the only game on a December date; it does not count in
    // the standings and both teams finish with 83 games.
    let mut per_date: HashMap<NaiveDate, usize> = HashMap::new();
    for g in games.iter().filter(|g| !g.is_playoff()) {
        *per_date.entry(g.game_date).or_insert(0) += 1;
    }
    let mut totals: HashMap<i32, i32> = HashMap::new();
    for g in games.iter().filter(|g| !g.is_playoff() && g.is_played()) {
        *totals.entry(g.home_team_id).or_insert(0) += 1;
        *totals.entry(g.away_team_id).or_insert(0) += 1;
    }
    let is_cup_final = |g: &Game| {
        season >= 2024
            && g.game_date.format("%m").to_string() == "12"
            && per_date.get(&g.game_date) == Some(&1)
            && totals.get(&g.home_team_id).copied().unwrap_or(0) >= 83
            && totals.get(&g.away_team_id).copied().unwrap_or(0) >= 83
    };
    if season == 2020 {
        let d = NaiveDate::from_ymd_opt(2020, 8, 15).expect("valid date");
        for g in games.iter().filter(|g| !g.is_playoff()) {
            if g.game_date == d {
                play_in.push(g.id);
            } else {
                regular.push(g.id);
            }
        }
        return (play_in, cup_final, regular);
    }
    if season < 2021 {
        return (vec![], vec![], games.iter().filter(|g| !g.is_playoff()).map(|g| g.id).collect());
    }
    // Teams not in the play-in play exactly the regular-season length; anything beyond is play-in.
    let mut count: HashMap<i32, i32> = HashMap::new();
    for g in games.iter().filter(|g| !g.is_playoff() && g.is_played() && !is_cup_final(g)) {
        *count.entry(g.home_team_id).or_insert(0) += 1;
        *count.entry(g.away_team_id).or_insert(0) += 1;
    }
    let played: Vec<i32> = count.values().copied().collect();
    let season_len = played.iter().copied().min().unwrap_or(0);
    // In-progress seasons: no team has finished, so nothing is a play-in game yet.
    let finished = played.iter().filter(|&&c| c >= season_len).count() >= 20 && season_len >= 72;
    let mut index: HashMap<i32, i32> = HashMap::new();
    for g in games.iter().filter(|g| !g.is_playoff()) {
        if is_cup_final(g) {
            cup_final.push(g.id);
            continue;
        }
        let hi = index.entry(g.home_team_id).or_insert(0);
        *hi += 1;
        let h_idx = *hi;
        let ai = index.entry(g.away_team_id).or_insert(0);
        *ai += 1;
        let a_idx = *ai;
        if finished && (h_idx > season_len || a_idx > season_len) {
            play_in.push(g.id);
        } else {
            regular.push(g.id);
        }
    }
    (play_in, cup_final, regular)
}

pub async fn rebuild_seasons(pool: &Pool, seasons: &[i32]) -> anyhow::Result<()> {
    let teams = db::teams::team_map(pool).await?;
    let curated: HashMap<i32, (&str, &str)> = reference::CURATED_CHAMPIONS.iter().map(|c| (c.0, (c.1, c.2))).collect();
    for &season in seasons {
        let mut games = db::games::season_games(pool, season).await?;
        // 1. play-in classification
        let (pi, cf, reg) = classify_non_playoff(season, &games);
        let type_of: HashMap<i64, GameType> = games.iter().map(|g| (g.id, g.game_type)).collect();
        let mut changed = false;
        for (ids, wanted) in [(pi, GameType::PlayIn), (cf, GameType::CupFinal), (reg, GameType::Regular)] {
            let ids: Vec<i64> = ids.into_iter().filter(|id| type_of.get(id) != Some(&wanted)).collect();
            if !ids.is_empty() {
                sqlx::query("UPDATE games SET game_type = $2 WHERE id = ANY($1)").bind(&ids).bind(wanted).execute(pool).await?;
                changed = true;
            }
        }
        if changed {
            games = db::games::season_games(pool, season).await?;
        }
        // 2. series / results
        let series = reconstruct::series_from_games(season, &games);
        let complete = reconstruct::postseason_complete(&series);
        let results = reconstruct::playoff_results(&series);
        let seeds: HashMap<i32, i32> = match reconstruct::reconstruct(season, &games, &teams) {
            Ok(r) => r.bracket.teams.iter().map(|t| (t.team_id, t.seed as i32)).collect(),
            Err(_) => HashMap::new(),
        };
        // 3. seasons row
        let regular_end = games.iter().filter(|g| g.game_type == GameType::Regular && g.is_played()).map(|g| g.game_date).max();
        let po_start = games.iter().filter(|g| g.is_playoff()).map(|g| g.game_date).min();
        let po_end = games.iter().filter(|g| g.is_playoff()).map(|g| g.game_date).max();
        let mut team_ids: Vec<i32> = games.iter().flat_map(|g| [g.home_team_id, g.away_team_id]).collect();
        team_ids.sort();
        team_ids.dedup();
        sqlx::query(
            "UPDATE seasons SET team_count = $2, game_count = $3, playoff_game_count = $4, play_in_game_count = $5,
                regular_season_end = $6, playoffs_start = $7, playoffs_end = $8, complete = $9, cup_final_game_count = $10,
                updated_at = now() WHERE season = $1",
        )
        .bind(season)
        .bind(team_ids.len() as i32)
        .bind(games.len() as i32)
        .bind(games.iter().filter(|g| g.is_playoff()).count() as i32)
        .bind(games.iter().filter(|g| g.game_type == GameType::PlayIn).count() as i32)
        .bind(regular_end)
        .bind(po_start)
        .bind(po_end)
        .bind(complete)
        .bind(games.iter().filter(|g| g.game_type == GameType::CupFinal).count() as i32)
        .execute(pool)
        .await?;
        // 4. team seasons
        let alignment = reconstruct::alignment_map(season, &teams, &games);
        let play_in_teams: std::collections::HashSet<i32> =
            games.iter().filter(|g| g.game_type == GameType::PlayIn).flat_map(|g| [g.home_team_id, g.away_team_id]).collect();
        let playoff_teams: std::collections::HashSet<i32> =
            games.iter().filter(|g| g.is_playoff()).flat_map(|g| [g.home_team_id, g.away_team_id]).collect();
        for &tid in &team_ids {
            let Some(team) = teams.get(&tid) else { continue };
            let ts = team_season(season, team, &games, &alignment, &results, &seeds, &play_in_teams, &playoff_teams);
            db::analysis::upsert_team_season(pool, &ts).await?;
        }
        // 5. championship
        if let Some((w, l)) = reconstruct::champion(&series) {
            let fran = teams[&w].franchise_id;
            db::analysis::upsert_championship(pool, season, w, fran, Some(l), "derived").await?;
            if let Some((ck, _)) = curated.get(&season) {
                let wk = reference::lookup_team(&teams[&w].code).map(|t| t.1).unwrap_or("");
                if wk != *ck {
                    warn!(season, derived = wk, curated = ck, "derived champion disagrees with curated record");
                }
            }
        } else if let Some((ck, rk)) = curated.get(&season) {
            // No complete playoff data: fall back to the curated record (never invent Elo).
            let champ = teams.values().find(|t| &t.code == ck);
            let ru = teams.values().find(|t| &t.code == rk);
            if let Some(c) = champ {
                db::analysis::upsert_championship(pool, season, c.id, c.franchise_id, ru.map(|t| t.id), "curated").await?;
            }
        }
        info!(season, games = games.len(), complete, "derived data rebuilt");
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn team_season(
    season: i32,
    team: &Team,
    games: &[Game],
    alignment: &HashMap<i32, (Conference, String)>,
    results: &HashMap<i32, PlayoffResult>,
    seeds: &HashMap<i32, i32>,
    play_in_teams: &std::collections::HashSet<i32>,
    playoff_teams: &std::collections::HashSet<i32>,
) -> TeamSeason {
    let tid = team.id;
    let (mut wins, mut losses) = (0, 0);
    let mut elo_start = None;
    let mut elo_regular_end = None;
    let mut elo_pre_playoffs = None;
    let mut elo_end = None;
    let mut elo_peak: Option<f64> = None;
    for g in games.iter().filter(|g| g.involves(tid)) {
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
    let (conference, division) = alignment.get(&tid).map_or((None, None), |(c, d)| (Some(*c), Some(d.clone())));
    let made_playoffs = playoff_teams.contains(&tid);
    let made_play_in = play_in_teams.contains(&tid);
    let playoff_result = results.get(&tid).copied().unwrap_or(if made_play_in { PlayoffResult::PlayIn } else { PlayoffResult::Missed });
    TeamSeason {
        season,
        team_id: tid,
        franchise_id: team.franchise_id,
        conference,
        division,
        wins,
        losses,
        win_pct: if wins + losses > 0 { wins as f64 / (wins + losses) as f64 } else { 0.0 },
        elo_start,
        elo_regular_end,
        elo_pre_playoffs,
        elo_end,
        elo_peak,
        made_play_in,
        made_playoffs,
        seed: seeds.get(&tid).copied(),
        playoff_result,
    }
}
