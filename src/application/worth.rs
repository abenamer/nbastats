//! Player worth per season and the availability of the current rosters, from the stored
//! games, box-score lines and injury reports; see `domain::worth`.

use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::sync::Arc;

use chrono::NaiveDate;
use serde::Serialize;

use crate::domain::ids::{PlayerId, SeasonYear, TeamId};
use crate::domain::worth::{
    self, Absence, AbsenceStatus, PlayerWorth, ReportedAbsence, WorthParams, WorthTable,
};

use super::ports::{GameLog, Player, PlayerStore, StoreError};
use super::state::{App, SeasonLoadError};

/// A report is priced with the estimate of the smallest error among the season's own table
/// and those of this many earlier seasons.
pub const WORTH_LOOKBACK: usize = 2;

/// A player's worth with his name, for the page.
#[derive(Debug, Clone, Serialize)]
pub struct NamedWorth {
    #[serde(flatten)]
    pub worth: PlayerWorth,
    pub name: String,
    pub team_codes: Vec<String>,
}

/// One line of the current injury report with the worth the forecast takes off.
#[derive(Debug, Clone, Serialize)]
pub struct CurrentAbsence {
    pub player_id: PlayerId,
    pub name: String,
    pub team_id: TeamId,
    pub team_code: String,
    pub status: AbsenceStatus,
    pub status_text: String,
    pub return_date: Option<NaiveDate>,
    /// The worth used, in Elo, and the season it comes from; `None` without an estimate.
    pub worth_elo: Option<f64>,
    pub worth_season: Option<SeasonYear>,
    /// The penalty the forecast applies, when any: the worth weighted by the status.
    pub penalty_elo: Option<f64>,
    pub until: Option<NaiveDate>,
}

/// The availability of a season's rosters: the latest injury report priced in Elo.
#[derive(Debug, Clone, Serialize)]
pub struct Availability {
    pub season: SeasonYear,
    pub report_date: Option<NaiveDate>,
    pub season_end: NaiveDate,
    pub lines: Vec<CurrentAbsence>,
    pub absences: Vec<Absence>,
    /// Why nothing is priced, when nothing is.
    pub note: Option<String>,
}

/// The worth table of a season, cached until the next import or sync.
pub async fn table(app: &App, season: SeasonYear) -> Result<Arc<WorthTable>, SeasonLoadError> {
    if let Some(t) = app
        .worths
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .get(&season)
        .cloned()
    {
        return Ok(t);
    }
    let generation = app.cache_generation.load(Ordering::SeqCst);
    let data = app.season_data(season).await?;
    let lines = app.store.season_lines(season).await?;
    // A Cholesky over a few hundred players: tens of milliseconds, off the request thread.
    let computed = tokio::task::spawn_blocking(move || {
        Arc::new(worth::table(
            season,
            &data.games,
            &lines,
            WorthParams::default(),
        ))
    })
    .await
    .map_err(|e| SeasonLoadError::Store(StoreError::Query(e.to_string())))?;
    let mut cache = app.worths.write().unwrap_or_else(|e| e.into_inner());
    if app.cache_generation.load(Ordering::SeqCst) == generation {
        cache.insert(season, computed.clone());
    }
    Ok(computed)
}

/// The worth table with the players' names and team codes.
pub async fn named_table(
    app: &App,
    season: SeasonYear,
) -> Result<(Arc<WorthTable>, Vec<NamedWorth>), SeasonLoadError> {
    let t = table(app, season).await?;
    let ids: Vec<PlayerId> = t.players.iter().map(|w| w.player_id).collect();
    let players = names(app, &ids).await?;
    let teams = app.teams().await?;
    let rows = t
        .players
        .iter()
        .map(|w| NamedWorth {
            worth: w.clone(),
            name: players
                .get(&w.player_id)
                .map(full_name)
                .unwrap_or_else(|| w.player_id.to_string()),
            team_codes: w
                .teams
                .iter()
                .filter_map(|t| teams.get(t).map(|team| team.code.clone()))
                .collect(),
        })
        .collect();
    Ok((t, rows))
}

async fn names(app: &App, ids: &[PlayerId]) -> Result<HashMap<PlayerId, Player>, StoreError> {
    Ok(app
        .store
        .players_by_id(ids)
        .await?
        .into_iter()
        .map(|p| (p.id, p))
        .collect())
}

fn full_name(p: &Player) -> String {
    format!("{} {}", p.first_name, p.last_name)
        .trim()
        .to_string()
}

/// The tables a report is priced against: the season's and the `WORTH_LOOKBACK` before
/// it, oldest last; a season without games is left out.
async fn lookback_tables(
    app: &App,
    season: SeasonYear,
) -> Result<Vec<(SeasonYear, Arc<WorthTable>)>, SeasonLoadError> {
    let mut out = Vec::new();
    let mut s = Some(season);
    for _ in 0..=WORTH_LOOKBACK {
        let Some(current) = s else { break };
        match table(app, current).await {
            Ok(t) => out.push((current, t)),
            Err(SeasonLoadError::NoGames(_)) => {}
            Err(e) => return Err(e),
        }
        s = current.previous();
    }
    Ok(out)
}

/// The estimate to price a player with, among the tables given: the one with the
/// smallest standard error, so a season with one absence does not displace a full
/// season before it. Ties go to the latest season.
pub fn pick_estimate(
    tables: &[(SeasonYear, Arc<WorthTable>)],
    player: PlayerId,
) -> Option<(f64, SeasonYear)> {
    tables
        .iter()
        .filter_map(|(season, t)| {
            t.players
                .iter()
                .find(|w| w.player_id == player)
                .map(|w| (w.se_points, *season, w.worth_elo))
        })
        .min_by(|a, b| a.0.total_cmp(&b.0).then(b.1.cmp(&a.1)))
        .map(|(_, season, elo)| (elo, season))
}

/// A report older than this before the season's first game, or dated after its last game,
/// is not priced: it describes another season.
pub const REPORT_WINDOW_DAYS: i64 = 30;

/// The latest injury report priced for `season`: each line's worth, status weight and span.
/// A report that does not belong to the season (dated more than `REPORT_WINDOW_DAYS` before
/// its first game, or after its last) prices nothing, and `note` says so.
pub async fn availability(app: &App, season: SeasonYear) -> Result<Availability, SeasonLoadError> {
    let data = app.season_data(season).await?;
    let season_end = data
        .games
        .iter()
        .map(|g| g.game_date)
        .max()
        .unwrap_or_else(|| app.today());
    let season_start = data
        .games
        .iter()
        .map(|g| g.game_date)
        .min()
        .unwrap_or(season_end);
    let empty = |note: Option<String>, report_date: Option<NaiveDate>| Availability {
        season,
        report_date,
        season_end,
        lines: vec![],
        absences: vec![],
        note,
    };
    let Some(report_date) = app.store.latest_injury_report_date().await? else {
        return Ok(empty(Some("No injury report is stored.".to_string()), None));
    };
    if report_date < season_start - chrono::Duration::days(REPORT_WINDOW_DAYS)
        || report_date > season_end
    {
        return Ok(empty(
            Some(format!(
                "The latest injury report is dated {report_date}, outside this season; it is not priced."
            )),
            Some(report_date),
        ));
    }
    let tables = lookback_tables(app, season).await?;
    let report = app.store.injury_report(report_date).await?;
    let teams = app.store.team_code_map().await?;
    let mut lines = Vec::with_capacity(report.len());
    let mut reported = Vec::new();
    for l in report {
        let Some(team) = teams.get(&l.team_code) else {
            continue;
        };
        let status = AbsenceStatus::parse(&l.status);
        let found = pick_estimate(&tables, l.player_id);
        let entry = found.map(|(w, _)| ReportedAbsence {
            player_id: l.player_id,
            team_id: team.id,
            status,
            return_date: l.return_date,
            worth_elo: w,
        });
        let applied = entry
            .as_ref()
            .map(|r| worth::absences_from_report(std::slice::from_ref(r), report_date, season_end))
            .unwrap_or_default();
        let one = applied.first();
        lines.push(CurrentAbsence {
            player_id: l.player_id,
            name: l.player_name.clone(),
            team_id: team.id,
            team_code: l.team_code.clone(),
            status,
            status_text: l.status.clone(),
            return_date: l.return_date,
            worth_elo: found.map(|(w, _)| w),
            worth_season: found.map(|(_, s)| s),
            penalty_elo: one.map(|a| a.elo),
            until: one.map(|a| a.to),
        });
        if let Some(r) = entry {
            reported.push(r);
        }
    }
    let absences = worth::absences_from_report(&reported, report_date, season_end);
    lines.sort_by(|a, b| {
        b.penalty_elo
            .unwrap_or(0.0)
            .total_cmp(&a.penalty_elo.unwrap_or(0.0))
            .then_with(|| a.team_code.cmp(&b.team_code))
            .then_with(|| a.name.cmp(&b.name))
    });
    Ok(Availability {
        season,
        report_date: Some(report_date),
        season_end,
        lines,
        absences,
        note: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::worth::{PlayerWorth, WorthParams};

    fn table_with(season: i32, se: f64, elo: f64) -> (SeasonYear, Arc<WorthTable>) {
        let w = PlayerWorth {
            player_id: PlayerId::new(1),
            teams: vec![],
            games_played: 60,
            absences: 10,
            worth_points: elo / 28.0,
            worth_elo: elo,
            se_points: se,
            se_elo: se * 28.0,
            with_mean: 0.0,
            without_mean: 0.0,
        };
        (
            SeasonYear::of(season),
            Arc::new(WorthTable {
                season: SeasonYear::of(season),
                params: WorthParams::default(),
                prior_sd_points: 3.0,
                prior_estimated: false,
                games: 1,
                games_with_absence: 1,
                residual_sd_points: 12.0,
                players_seen: 1,
                players: vec![w],
            }),
        )
    }

    #[test]
    fn the_estimate_with_the_smallest_error_wins_and_ties_go_to_the_latest_season() {
        let tables = vec![
            table_with(2026, 3.5, 10.0),
            table_with(2025, 1.2, 120.0),
            table_with(2024, 1.2, 90.0),
        ];
        assert_eq!(
            pick_estimate(&tables, PlayerId::new(1)),
            Some((120.0, SeasonYear::of(2025)))
        );
        assert_eq!(pick_estimate(&tables, PlayerId::new(2)), None);
        assert_eq!(pick_estimate(&[], PlayerId::new(1)), None);
    }
}
