//! The career worth ranking: the players since 1976-77 in tiers, each from the box score of
//! their career and from the gap between their teams' results in the games they played and
//! the games they missed; see `domain::career`. The report is computed from the stored games
//! and box scores, kept until the next import or player sync, and computed again on the
//! first read after it.

use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::sync::Arc;

use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use serde::Serialize;

use crate::domain::career::{
    self, BoxSeason, Calibration, Career, CellRule, HalfCheck, Ranked, Ranking, Score, Settings,
    GROUPS,
};
use crate::domain::ids::{PlayerId, SeasonYear};

use super::boxscore;
use super::ports::{GameLog, PlayerStore, StoreError};
use super::state::{App, SeasonLoadError};

/// A row with the player's name, for the page.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Named<T> {
    pub name: String,
    /// Listed on a team by the last roster refresh.
    pub active: bool,
    #[serde(flatten)]
    pub row: T,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CareerReport {
    pub model: &'static str,
    pub rule: CellRule,
    pub settings: Settings,
    /// The seasons whose box scores gave a box worth, first and last.
    pub box_seasons: Option<(SeasonYear, SeasonYear)>,
    /// Players with a stint.
    pub measured: usize,
    /// Players with the games the settings ask and a career box worth: the ranks and the
    /// chances are among these.
    pub ranked: usize,
    /// Players with fewer games as a regular than the settings ask: not reported.
    pub short_careers: usize,
    /// Players with the games and no box worth: not ranked.
    pub without_box: usize,
    /// Ranked players whose teams played no game without them inside a stint.
    pub without_gap: usize,
    pub calibration: Option<Calibration>,
    pub spread: f64,
    pub typical_error: f64,
    pub typical_gap_error: f64,
    pub expected_in_list: f64,
    pub tier_sizes: [usize; 5],
    /// Every ranked player in rank order; the list is the first `settings.listed`.
    pub players: Vec<Named<Ranked>>,
    pub half_check: Option<HalfCheck>,
}

/// The settings a run may take: at least one draw and one listed player, and no more draws
/// than a request should cost.
pub fn check(settings: &Settings) -> Result<(), String> {
    if !(1..=200_000).contains(&settings.draws) {
        return Err("the draws must be between 1 and 200000".to_string());
    }
    if !(1..=1_000).contains(&settings.listed) {
        return Err("the list must hold between 1 and 1000 players".to_string());
    }
    Ok(())
}

/// Every player's box worth in every season from `first` to the latest stored one: one
/// line for each team he played for. A season without games, or whose box scores cannot
/// carry the box plus-minus, gives no line.
async fn box_seasons(app: &App, first: SeasonYear) -> Result<Vec<BoxSeason>, SeasonLoadError> {
    let Some(latest) = app.store.latest_season().await? else {
        return Ok(Vec::new());
    };
    let mut out = Vec::new();
    let mut season = Some(first);
    while let Some(current) = season.filter(|s| *s <= latest) {
        match boxscore::table(app, current).await {
            Ok(t) if t.bpm_available => out.extend(t.players.iter().map(|p| BoxSeason {
                player_id: p.player_id,
                season: current,
                minutes: p.minutes,
                worth_elo: p.worth_elo,
            })),
            Ok(_) | Err(SeasonLoadError::NoGames(_)) => {}
            Err(e) => return Err(e),
        }
        season = current.next();
    }
    Ok(out)
}

/// The report under `settings`, computed now from the store.
pub async fn compute(app: &App, settings: Settings) -> Result<CareerReport, SeasonLoadError> {
    let rule = CellRule::default();
    let cells = app.store.career_cells(&rule).await?;
    let boxes = box_seasons(app, rule.first_season).await?;
    let box_seasons = boxes
        .iter()
        .map(|b| b.season)
        .min()
        .zip(boxes.iter().map(|b| b.season).max());
    // Twenty thousand draws over a few hundred players, and the half-career check: off the
    // request thread.
    let ranking: Ranking = tokio::task::spawn_blocking(move || {
        let mut rng = ChaCha8Rng::seed_from_u64(settings.seed);
        career::rank(
            career::measure(&cells, rule.elo_per_point),
            &career::career_box(&boxes),
            settings,
            career::half_check(&cells, &boxes, settings.min_games, rule.elo_per_point),
            &mut rng,
        )
    })
    .await
    .map_err(|e| StoreError::Query(e.to_string()))?;
    let ids: Vec<PlayerId> = ranking.players.iter().map(|r| r.career.player_id).collect();
    let names: HashMap<PlayerId, (String, bool)> = app
        .store
        .players_by_id(&ids)
        .await?
        .into_iter()
        .map(|p| {
            // A few stored names carry a comma between a nickname and the family name.
            let name = format!("{} {}", p.first_name, p.last_name).replace(',', " ");
            let name = name.split_whitespace().collect::<Vec<_>>().join(" ");
            (p.id, (name, p.active))
        })
        .collect();
    let named = |id: PlayerId| {
        names
            .get(&id)
            .cloned()
            .unwrap_or_else(|| (id.to_string(), false))
    };
    Ok(CareerReport {
        model: ranking.model,
        rule,
        settings: ranking.settings,
        box_seasons,
        measured: ranking.measured,
        ranked: ranking.ranked,
        short_careers: ranking.short_careers,
        without_box: ranking.without_box,
        without_gap: ranking.without_gap,
        calibration: ranking.calibration,
        spread: ranking.spread,
        typical_error: ranking.typical_error,
        typical_gap_error: ranking.typical_gap_error,
        expected_in_list: ranking.expected_in_list,
        tier_sizes: ranking.tier_sizes,
        half_check: ranking.half_check,
        players: ranking
            .players
            .into_iter()
            .map(|row| {
                let (name, active) = named(row.career.player_id);
                Named { name, active, row }
            })
            .collect(),
    })
}

/// The report under the default settings, kept until the next import or player sync.
pub async fn report(app: &App) -> Result<Arc<CareerReport>, SeasonLoadError> {
    if let Some(r) = app.career.read().unwrap_or_else(|e| e.into_inner()).clone() {
        return Ok(r);
    }
    let generation = app.cache_generation.load(Ordering::SeqCst);
    let computed = Arc::new(compute(app, Settings::default()).await?);
    let mut cache = app.career.write().unwrap_or_else(|e| e.into_inner());
    if app.cache_generation.load(Ordering::SeqCst) == generation {
        *cache = Some(computed.clone());
    }
    Ok(computed)
}

fn seasons(c: &Career) -> String {
    format!("{}–{}", c.first_season.label(), c.last_season.label())
}

fn share(p: f64) -> String {
    format!("{:.0}%", p * 100.0)
}

fn score_row(name: &str, s: Score) -> String {
    format!(
        "| {} | {:.3} | {:.1} | {:.2} |\n",
        name, s.correlation, s.excess_error, s.z_spread
    )
}

/// The report as Markdown, for the command line and the decision record.
pub fn render(r: &CareerReport) -> String {
    let mut out = format!(
        "# Career worth, model {}\n\nSeasons from {}. A stint: at least {} games and {} minutes a game. Ranked: {} games as a regular and a career box worth. {} draws, seed {}.\n\n",
        r.model,
        r.rule.first_season.label(),
        r.rule.min_games,
        r.rule.min_minutes,
        r.settings.min_games,
        r.settings.draws,
        r.settings.seed,
    );
    if let Some((first, last)) = r.box_seasons {
        out.push_str(&format!(
            "Box worths from the seasons {} to {}.\n\n",
            first.label(),
            last.label()
        ));
    }
    let list = &r.players[..r.players.len().min(r.settings.listed)];
    out.push_str(&format!(
        "Players with a stint {}; ranked {} ({} without a gap); {} long careers without a box worth; {} short careers. Spread of the worths {:.1} Elo, typical error {:.1}, typical error of a gap {:.1}. Tiers among the listed {}: {:?}. Expected members of the best {} among the listed: {:.1}.\n\n",
        r.measured,
        r.ranked,
        r.without_gap,
        r.without_box,
        r.short_careers,
        r.spread,
        r.typical_error,
        r.typical_gap_error,
        list.len(),
        r.tier_sizes,
        GROUPS
            .iter()
            .find(|g| **g >= r.settings.listed)
            .unwrap_or(&GROUPS[GROUPS.len() - 1]),
        r.expected_in_list,
    ));
    match r.calibration {
        Some(c) => out.push_str(&format!(
            "Calibration over {} players: gap = {:.1} + {:.3} (± {:.3}) × box worth; spread around the line {:.1} Elo; mean box worth {:.1}, error of the line there {:.1}.\n\n",
            c.players, c.intercept, c.slope, c.slope_error, c.spread, c.mean_box, c.mean_error,
        )),
        None => out.push_str("No calibration: too few players with a gap and a box worth.\n\n"),
    }
    if let Some(h) = r.half_check {
        out.push_str(&format!(
            "Each estimate built on one half of the seasons (odd or even) and scored against the gaps of the other half, {} player halves:\n\n| Estimate | Correlation | Miss beyond the errors (Elo) | Spread of the misses over their errors |\n|---|---|---|---|\n",
            h.pairs,
        ));
        out.push_str(&score_row("The gap alone, shrunk", h.gap));
        out.push_str(&score_row("The box prior alone", h.prior));
        out.push_str(&score_row("The career worth", h.blend));
        out.push('\n');
    }
    out.push_str("| Rank | Tier | Player | Seasons | Worth | ± | Box worth | Prior | ± | Gap | ± | Expected rank | Top 10 | Top 25 | Top 50 | Top 100 | Games | Missed | Dominant season |\n|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|\n");
    for Named { name, row, .. } in list {
        let c = &row.career;
        let gap = |f: fn(&career::Gap) -> String| c.gap.as_ref().map_or(String::new(), f);
        out.push_str(&format!(
            "| {} | {} | {} | {} | {:.0} | {:.0} | {:.0} | {:.0} | {:.0} | {} | {} | {:.1} | {} | {} | {} | {} | {} | {} | {} |\n",
            row.rank,
            row.tier.number(),
            name,
            seasons(c),
            row.worth,
            row.error,
            row.box_worth,
            row.prior,
            row.prior_error,
            gap(|g| format!("{:.0}", g.worth)),
            gap(|g| format!("{:.0}", g.error)),
            row.expected_rank,
            share(row.top10),
            share(row.top25),
            share(row.top50),
            share(row.top100),
            c.career_games,
            gap(|g| g.missed.to_string()),
            gap(|g| g.dominant_season.map_or(String::new(), |s| s.label())),
        ));
    }
    out
}
