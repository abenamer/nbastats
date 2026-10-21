//! Dashboard aggregates: historical title distribution and concentration versus theory.

use std::collections::BTreeMap;

use serde::Serialize;

use crate::app::App;
use crate::db;
use crate::simulation::alpha::{self, AlphaParams};
use crate::simulation::exact;
use crate::simulation::progress::Silent;
use crate::simulation::stats;

#[derive(Debug, Clone, Serialize)]
pub struct FranchiseTitles {
    pub key: String,
    pub name: String,
    pub titles: i64,
    pub seasons: Vec<i32>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DashboardData {
    pub from_season: i32,
    pub to_season: i32,
    pub seasons_with_champion: usize,
    pub franchise_count: usize,
    pub titles: Vec<FranchiseTitles>,
    pub champions: Vec<db_view::ChampionRow>,
    pub gini: f64,
    pub hhi: f64,
    pub top5_share: f64,
    pub max_titles: i64,
    /// Observed number of franchises with exactly j titles.
    pub observed_by_count: Vec<usize>,
    /// Expected number of franchises with exactly j titles under equal strength.
    pub expected_by_count_equal: Vec<f64>,
    pub p_any_at_least_max_equal: f64,
    pub p_any_at_least_4_equal: f64,
    pub expected_gini_equal: f64,
    pub expected_hhi_equal: f64,
    pub expected_top5_equal: f64,
    pub latest_season: Option<i32>,
    pub game_count: i64,
}

pub mod db_view {
    #[derive(Debug, Clone, serde::Serialize)]
    pub struct ChampionRow {
        pub season: i32,
        pub label: String,
        pub champion: String,
        pub champion_name: String,
        pub runner_up: Option<String>,
        pub source: String,
    }
}

pub async fn build(app: &App, from: i32, to: i32) -> anyhow::Result<DashboardData> {
    let franchises = db::teams::active_franchises(&app.pool).await?;
    let champions = db::analysis::championships(&app.pool, from, to).await?;
    let mut by_key: BTreeMap<String, FranchiseTitles> = franchises
        .iter()
        .map(|f| (f.key.clone(), FranchiseTitles { key: f.key.clone(), name: f.name.clone(), titles: 0, seasons: vec![] }))
        .collect();
    for c in &champions {
        if let Some(f) = by_key.get_mut(&c.franchise_key) {
            f.titles += 1;
            f.seasons.push(c.season);
        }
    }
    let mut titles: Vec<FranchiseTitles> = by_key.into_values().collect();
    titles.sort_by(|a, b| b.titles.cmp(&a.titles).then_with(|| a.key.cmp(&b.key)));
    let counts: Vec<f64> = titles.iter().map(|t| t.titles as f64).collect();
    let n_seasons = champions.len();
    let n_fr = titles.len().max(1);
    let max_titles = titles.iter().map(|t| t.titles).max().unwrap_or(0);
    let mut observed_by_count = vec![0usize; n_seasons + 1];
    for t in &titles {
        observed_by_count[t.titles as usize] += 1;
    }
    let equal = vec![1.0; n_fr];
    let (expected_by_count_equal, eg, eh, et) = if n_seasons > 0 {
        let p = AlphaParams::equal(n_fr, n_seasons, 20_000, 1, 7);
        let r = tokio::task::spawn_blocking(move || alpha::run(&p, &Silent)).await?;
        (r.franchises_by_title_count, r.gini_mean, r.hhi_mean, r.top5_share_mean)
    } else {
        (vec![], 0.0, 0.0, 0.0)
    };
    Ok(DashboardData {
        from_season: from,
        to_season: to,
        seasons_with_champion: n_seasons,
        franchise_count: n_fr,
        champions: champions
            .iter()
            .map(|c| db_view::ChampionRow {
                season: c.season,
                label: crate::reference::season_label(c.season),
                champion: c.franchise_key.clone(),
                champion_name: c.champion_name.clone(),
                runner_up: c.runner_up_code.clone(),
                source: c.source.clone(),
            })
            .collect(),
        titles,
        gini: stats::gini(&counts),
        hhi: stats::hhi(&counts),
        top5_share: stats::top_k_share(&counts, 5),
        max_titles,
        observed_by_count,
        expected_by_count_equal,
        p_any_at_least_max_equal: if n_seasons > 0 { exact::p_any_at_least(&equal, n_seasons, max_titles.max(1) as usize) } else { 0.0 },
        p_any_at_least_4_equal: if n_seasons >= 4 { exact::p_any_at_least(&equal, n_seasons, 4) } else { 0.0 },
        expected_gini_equal: eg,
        expected_hhi_equal: eh,
        expected_top5_equal: et,
        latest_season: db::games::latest_season(&app.pool).await?,
        game_count: db::games::game_count(&app.pool).await?,
    })
}
