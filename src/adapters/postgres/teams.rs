use std::collections::HashMap;

use crate::domain::models::{Franchise, Team};
use crate::domain::reference;

use super::Pool;

/// Insert or update franchises and teams from the static lineage table. Idempotent.
pub async fn sync_reference(pool: &Pool) -> anyhow::Result<()> {
    let mut tx = pool.begin().await?;
    // Franchises
    let mut seen = std::collections::HashSet::new();
    for t in reference::TEAM_LINEAGE {
        if !seen.insert(t.1) {
            continue;
        }
        let active = reference::TEAM_LINEAGE.iter().any(|u| u.1 == t.1 && u.5 == 9999);
        sqlx::query(
            "INSERT INTO franchises (key, name, active) VALUES ($1, $2, $3)
             ON CONFLICT (key) DO UPDATE SET name = EXCLUDED.name, active = EXCLUDED.active",
        )
        .bind(t.1)
        .bind(t.2)
        .bind(active)
        .execute(&mut *tx)
        .await?;
    }
    for t in reference::TEAM_LINEAGE {
        let last: Option<i32> = if t.5 == 9999 { None } else { Some(t.5) };
        sqlx::query(
            "INSERT INTO teams (code, franchise_id, name, first_season, last_season)
             VALUES ($1, (SELECT id FROM franchises WHERE key = $2), $3, $4, $5)
             ON CONFLICT (code) DO UPDATE SET franchise_id = EXCLUDED.franchise_id, name = EXCLUDED.name,
               first_season = EXCLUDED.first_season, last_season = EXCLUDED.last_season",
        )
        .bind(t.0)
        .bind(t.1)
        .bind(t.3)
        .bind(t.4)
        .bind(last)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
}

pub async fn all_teams(pool: &Pool) -> anyhow::Result<Vec<Team>> {
    Ok(sqlx::query_as::<_, Team>("SELECT id, code, franchise_id, name, first_season, last_season FROM teams ORDER BY code")
        .fetch_all(pool)
        .await?)
}

pub async fn all_franchises(pool: &Pool) -> anyhow::Result<Vec<Franchise>> {
    Ok(sqlx::query_as::<_, Franchise>("SELECT id, key, name, active FROM franchises ORDER BY key")
        .fetch_all(pool)
        .await?)
}

pub async fn active_franchises(pool: &Pool) -> anyhow::Result<Vec<Franchise>> {
    Ok(sqlx::query_as::<_, Franchise>("SELECT id, key, name, active FROM franchises WHERE active ORDER BY key")
        .fetch_all(pool)
        .await?)
}

pub async fn franchise_by_key(pool: &Pool, key: &str) -> anyhow::Result<Option<Franchise>> {
    Ok(sqlx::query_as::<_, Franchise>("SELECT id, key, name, active FROM franchises WHERE key = $1")
        .bind(key)
        .fetch_optional(pool)
        .await?)
}

pub async fn team_map(pool: &Pool) -> anyhow::Result<HashMap<i32, Team>> {
    Ok(all_teams(pool).await?.into_iter().map(|t| (t.id, t)).collect())
}

pub async fn team_code_map(pool: &Pool) -> anyhow::Result<HashMap<String, Team>> {
    Ok(all_teams(pool).await?.into_iter().map(|t| (t.code.clone(), t)).collect())
}

/// Sync conference/division alignment rows for every (season, team) that has games.
pub async fn sync_alignments(pool: &Pool) -> anyhow::Result<()> {
    let teams = all_teams(pool).await?;
    let rows: Vec<(i32, i32)> = sqlx::query_as(
        "SELECT DISTINCT season, team_id FROM (
            SELECT season, home_team_id AS team_id FROM games
            UNION SELECT season, away_team_id FROM games) s WHERE season >= 2003",
    )
    .fetch_all(pool)
    .await?;
    let by_id: HashMap<i32, &Team> = teams.iter().map(|t| (t.id, t)).collect();
    let mut tx = pool.begin().await?;
    for (season, team_id) in rows {
        if let Some(team) = by_id.get(&team_id) {
            if let Some((conf, div)) = reference::alignment(season, &team.code) {
                sqlx::query(
                    "INSERT INTO alignments (season, team_id, conference, division) VALUES ($1, $2, $3, $4)
                     ON CONFLICT (season, team_id) DO UPDATE SET conference = EXCLUDED.conference, division = EXCLUDED.division",
                )
                .bind(season)
                .bind(team_id)
                .bind(conf)
                .bind(div)
                .execute(&mut *tx)
                .await?;
            }
        }
    }
    tx.commit().await?;
    Ok(())
}
