//! Player worth per season and the availability of the current rosters, from the stored
//! games, box-score lines and injury reports; see `domain::worth`.

use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::sync::Arc;

use chrono::NaiveDate;
use serde::Serialize;

use crate::domain::bpm::BoxTable;
use crate::domain::ids::{PlayerId, SeasonYear, TeamId};
use crate::domain::spell::{self, SeasonRuns, Survival};
use crate::domain::status::{self, StatusSample, StatusTable};
use crate::domain::worth::{
    self, Absence, AbsenceStatus, PlayerWorth, ReportedAbsence, SpanRule, SpanSource, WeightSource,
    WorthParams, WorthTable,
};

use super::boxscore;
use super::ports::{GameLog, Player, PlayerStore, StoreError};
use super::state::{App, SeasonLoadError};
use crate::domain::models::GameType;

/// A report is priced with the estimate of the smallest error among the season's own table
/// and those of this many earlier seasons.
pub const WORTH_LOOKBACK: usize = 2;
/// The survival curve of the runs of missed games is fitted on this many seasons before
/// the season priced. The status curves are fitted on the season's own reports and those
/// of the same seasons before.
pub const SPELL_SEASONS: usize = 5;

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
    /// The worth used, in Elo, its standard error and the season it comes from; `None`
    /// without an estimate.
    pub worth_elo: Option<f64>,
    pub se_elo: Option<f64>,
    pub worth_season: Option<SeasonYear>,
    /// Which estimate priced the line: the player worth from the missed games, or the
    /// box-score worth when the first is missing or within one error of zero.
    pub worth_source: Option<WorthSource>,
    /// The penalty the forecast applies, when any: the worth weighted by the status.
    pub penalty_elo: Option<f64>,
    /// The share of the worth the penalty takes off, and what set it: the measured chance
    /// that a line of this status, in this state, misses the next game, or the default.
    pub weight: Option<f64>,
    pub weight_source: Option<WeightSource>,
    pub until: Option<NaiveDate>,
    /// What set `until`.
    pub span: Option<SpanSource>,
    /// Team games missed in a row up to the latest game with box scores, plus the games
    /// the team played since without box scores; none when the team has none.
    pub missed_so_far: Option<u32>,
    /// The team's latest game with box scores; none when it has none.
    pub lines_through: Option<NaiveDate>,
    /// On the injury report; false for a player read from the box scores alone.
    pub listed: bool,
}

/// The survival curve the spans lean on, for the reader.
#[derive(Debug, Clone, Serialize)]
pub struct SurvivalSummary {
    pub model: &'static str,
    /// The seasons the runs come from.
    pub seasons: Vec<SeasonYear>,
    pub runs: u32,
    pub censored: u32,
    pub longest: u32,
    pub mean_run_games: f64,
    /// The expected remaining games for a run already zero to twenty games long.
    pub expected_remaining: Vec<f64>,
}

/// What a status has meant on the stored reports, for the reader: one row per status and
/// state with enough lines.
#[derive(Debug, Clone, Serialize)]
pub struct StatusCurveSummary {
    pub status: AbsenceStatus,
    /// The player had already missed his team's last game before the report.
    pub in_run: bool,
    /// Lines followed.
    pub samples: u32,
    /// The share of the lines whose player missed the next game: the weight used.
    pub weight: f64,
    pub default_weight: f64,
    /// The games missed over the next thirty, in expectation.
    pub expected_missed: f64,
    /// The span in games the weight is applied over.
    pub span_games: f64,
    pub default_days: i64,
    /// The share missing each of the next games, from the next.
    pub miss: Vec<f64>,
}

/// The status curves the weights and spans lean on.
#[derive(Debug, Clone, Serialize)]
pub struct StatusSummary {
    pub model: &'static str,
    /// The seasons whose reports were followed.
    pub seasons: Vec<SeasonYear>,
    /// Lines followed, over every status and both states.
    pub lines: u32,
    pub curves: Vec<StatusCurveSummary>,
}

/// The availability of a season's rosters: the latest injury report priced in Elo.
#[derive(Debug, Clone, Serialize)]
pub struct Availability {
    pub season: SeasonYear,
    pub report_date: Option<NaiveDate>,
    pub season_end: NaiveDate,
    pub lines: Vec<CurrentAbsence>,
    pub absences: Vec<Absence>,
    /// None when too few runs are stored: the status defaults price the spans.
    pub survival: Option<SurvivalSummary>,
    /// None when no stored report line could be followed through the game logs. A status
    /// without a curve keeps its default weight and span.
    pub statuses: Option<StatusSummary>,
    /// Lines added from the box scores: regular players in a run not on the report.
    pub unlisted: u32,
    /// Why nothing is priced, when nothing is.
    pub note: Option<String>,
}

/// The runs of missed games of a season, cached until the next import or sync. A season
/// without games is `NoGames`.
pub async fn spell_runs(app: &App, season: SeasonYear) -> Result<Arc<SeasonRuns>, SeasonLoadError> {
    if let Some(r) = app
        .spell_runs
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .get(&season)
        .cloned()
    {
        return Ok(r);
    }
    let generation = app.cache_generation.load(Ordering::SeqCst);
    let data = app.season_data(season).await?;
    let lines = app.store.season_box_lines(season).await?;
    let runs = tokio::task::spawn_blocking(move || {
        Arc::new(spell::season_runs(season, &data.games, &lines))
    })
    .await
    .map_err(|e| SeasonLoadError::Store(StoreError::Query(e.to_string())))?;
    let mut cache = app.spell_runs.write().unwrap_or_else(|e| e.into_inner());
    if app.cache_generation.load(Ordering::SeqCst) == generation {
        cache.insert(season, runs.clone());
    }
    Ok(runs)
}

/// The survival curve from the runs of the [`SPELL_SEASONS`] seasons before `season` that
/// have games; none below the minimum of runs.
async fn survival_before(
    app: &App,
    season: SeasonYear,
) -> Result<Option<(Survival, Vec<SeasonYear>)>, StoreError> {
    let mut runs = Vec::new();
    let mut seasons = Vec::new();
    let mut year = season;
    for _ in 0..SPELL_SEASONS {
        let Some(previous) = year.previous() else {
            break;
        };
        year = previous;
        match spell_runs(app, year).await {
            Ok(r) => {
                if !r.runs.is_empty() {
                    seasons.push(year);
                    runs.extend(r.runs.iter().copied());
                }
            }
            Err(SeasonLoadError::NoGames(_)) => continue,
            Err(SeasonLoadError::Store(e)) => return Err(e),
        }
    }
    seasons.reverse();
    Ok(Survival::fit(&runs).map(|s| (s, seasons)))
}

/// The report lines of a season followed through its game logs, cached until the next
/// import, results sync or player sync. The reports read are those dated from [`REPORT_WINDOW_DAYS`] before the
/// season's first game to its last. A season without games is `NoGames`.
pub async fn status_samples(
    app: &App,
    season: SeasonYear,
) -> Result<Arc<Vec<StatusSample>>, SeasonLoadError> {
    if let Some(r) = app
        .status_samples
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .get(&season)
        .cloned()
    {
        return Ok(r);
    }
    let generation = app.cache_generation.load(Ordering::SeqCst);
    let data = app.season_data(season).await?;
    let dates = data.games.iter().map(|g| g.game_date);
    let (Some(first), Some(last)) = (dates.clone().min(), dates.max()) else {
        return Ok(Arc::new(vec![]));
    };
    let reports = app
        .store
        .injury_reports_between(first - chrono::Duration::days(REPORT_WINDOW_DAYS), last)
        .await?;
    let samples = if reports.is_empty() {
        Arc::new(vec![])
    } else {
        let lines = app.store.season_box_lines(season).await?;
        tokio::task::spawn_blocking(move || {
            Arc::new(status::samples(&data.games, &lines, &reports))
        })
        .await
        .map_err(|e| SeasonLoadError::Store(StoreError::Query(e.to_string())))?
    };
    let mut cache = app
        .status_samples
        .write()
        .unwrap_or_else(|e| e.into_inner());
    if app.cache_generation.load(Ordering::SeqCst) == generation {
        cache.insert(season, samples.clone());
    }
    Ok(samples)
}

/// The status curves from the report lines of `season` and the [`SPELL_SEASONS`] seasons
/// before it that have games; none when no line was followed.
async fn status_table(
    app: &App,
    season: SeasonYear,
) -> Result<Option<(StatusTable, Vec<SeasonYear>)>, StoreError> {
    let mut samples = Vec::new();
    let mut seasons = Vec::new();
    let mut year = Some(season);
    for _ in 0..=SPELL_SEASONS {
        let Some(y) = year else {
            break;
        };
        year = y.previous();
        match status_samples(app, y).await {
            Ok(s) => {
                if !s.is_empty() {
                    seasons.push(y);
                    samples.extend(s.iter().cloned());
                }
            }
            Err(SeasonLoadError::NoGames(_)) => continue,
            Err(SeasonLoadError::Store(e)) => return Err(e),
        }
    }
    seasons.reverse();
    if samples.is_empty() {
        return Ok(None);
    }
    let table = tokio::task::spawn_blocking(move || StatusTable::fit(&samples))
        .await
        .map_err(|e| StoreError::Query(e.to_string()))?;
    Ok(Some((table, seasons)))
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

/// The box-score tables of the season and the `WORTH_LOOKBACK` before it, latest first;
/// a season without games or without usable box scores is left out.
async fn box_lookback_tables(
    app: &App,
    season: SeasonYear,
) -> Result<Vec<(SeasonYear, Arc<BoxTable>)>, SeasonLoadError> {
    let mut out = Vec::new();
    let mut s = Some(season);
    for _ in 0..=WORTH_LOOKBACK {
        let Some(current) = s else { break };
        match boxscore::table(app, current).await {
            Ok(t) if t.bpm_available => out.push((current, t)),
            Ok(_) | Err(SeasonLoadError::NoGames(_)) => {}
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
) -> Option<Estimate> {
    tables
        .iter()
        .filter_map(|(season, t)| {
            t.players
                .iter()
                .find(|w| w.player_id == player)
                .map(|w| (w.se_points, *season, w.worth_elo, w.se_elo))
        })
        .min_by(|a, b| a.0.total_cmp(&b.0).then(b.1.cmp(&a.1)))
        .map(|(_, season, elo, se_elo)| Estimate {
            elo,
            se_elo,
            season,
            source: WorthSource::Worth,
        })
}

/// The worth estimate a report line is priced with.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Estimate {
    pub elo: f64,
    pub se_elo: f64,
    pub season: SeasonYear,
    pub source: WorthSource,
}

/// Where a worth estimate comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WorthSource {
    /// The regression on the games the team played without the player.
    Worth,
    /// Box Plus/Minus over replacement, per game played; it has no standard error.
    Box,
}

/// The box-score estimate for a player among the tables given (latest season first): the
/// box worth of his stint with the most minutes in the latest season that has one.
pub fn pick_box_estimate(
    tables: &[(SeasonYear, Arc<BoxTable>)],
    player: PlayerId,
) -> Option<Estimate> {
    tables.iter().find_map(|(season, t)| {
        t.players
            .iter()
            .filter(|p| p.player_id == player)
            .max_by(|a, b| a.minutes.total_cmp(&b.minutes))
            .map(|p| Estimate {
                elo: p.worth_elo,
                se_elo: 0.0,
                season: *season,
                source: WorthSource::Box,
            })
    })
}

/// The estimate to price a player with: the player worth when it stands at least one
/// standard error from zero, else the box-score worth when there is one, else whichever
/// exists.
pub fn choose_estimate(worth: Option<Estimate>, boxed: Option<Estimate>) -> Option<Estimate> {
    match (worth, boxed) {
        (Some(w), Some(b)) if w.elo.abs() < w.se_elo => Some(b),
        (Some(w), _) => Some(w),
        (None, b) => b,
    }
}

/// The absences a forecast takes when the injury report is used beside absences given by
/// hand: a given absence for a player replaces his report line; the rest of the report is
/// added after the given ones.
pub fn merge_with_report(mut given: Vec<Absence>, report: Vec<Absence>) -> Vec<Absence> {
    let players: Vec<PlayerId> = given.iter().filter_map(|a| a.player_id).collect();
    given.extend(
        report
            .into_iter()
            .filter(|a| a.player_id.is_none_or(|p| !players.contains(&p))),
    );
    given
}

/// Fix every penalty at its estimate: no error, no draw.
pub fn fix_penalties(absences: &mut [Absence]) {
    for a in absences {
        a.se_elo = 0.0;
    }
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
        survival: None,
        statuses: None,
        unlisted: 0,
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
    let box_tables = box_lookback_tables(app, season).await?;
    let report = app.store.injury_report(report_date).await?;
    let teams = app.store.team_code_map().await?;
    // What the game logs add: the runs in progress, and the curve of the runs before.
    let current = spell_runs(app, season).await?;
    let in_progress: HashMap<(TeamId, PlayerId), &spell::CurrentRun> = current
        .current
        .iter()
        .map(|c| ((c.team_id, c.player_id), c))
        .collect();
    let lines_through: HashMap<TeamId, NaiveDate> = current
        .lines_through
        .iter()
        .map(|t| (t.team_id, t.game_date))
        .collect();
    // Games a team played after its latest game with lines: nobody has a line for them,
    // so a run in progress ran through them.
    let since_lines = |team: TeamId| -> u32 {
        lines_through.get(&team).map_or(0, |through| {
            data.games
                .iter()
                .filter(|g| {
                    g.game_type == GameType::Regular
                        && g.is_played()
                        && g.involves(team)
                        && g.game_date > *through
                })
                .count() as u32
        })
    };
    let fitted = survival_before(app, season).await?;
    let mut upcoming: HashMap<TeamId, Vec<NaiveDate>> = HashMap::new();
    let mut unplayed: Vec<_> = data
        .games
        .iter()
        .filter(|g| {
            g.game_type == GameType::Regular && !g.is_played() && g.game_date >= report_date
        })
        .collect();
    unplayed.sort_by_key(|g| (g.game_date, g.seq, g.id));
    for g in unplayed {
        for t in [g.home_team_id, g.away_team_id] {
            upcoming.entry(t).or_default().push(g.game_date);
        }
    }
    let measured = status_table(app, season).await?;
    let rule = Some(SpanRule {
        survival: fitted.as_ref().map(|(s, _)| s),
        statuses: measured.as_ref().map(|(t, _)| t),
        upcoming: &upcoming,
    });
    let mut lines = Vec::with_capacity(report.len());
    let mut reported = Vec::new();
    let mut listed: std::collections::HashSet<PlayerId> = std::collections::HashSet::new();
    let mut price = |player_id: PlayerId,
                     name: String,
                     team_id: TeamId,
                     team_code: String,
                     status: AbsenceStatus,
                     status_text: String,
                     return_date: Option<NaiveDate>,
                     is_listed: bool| {
        let missed_so_far = lines_through.get(&team_id).map(|_| {
            in_progress
                .get(&(team_id, player_id))
                .map_or(0, |c| c.missed + since_lines(team_id))
        });
        let found = choose_estimate(
            pick_estimate(&tables, player_id),
            pick_box_estimate(&box_tables, player_id),
        );
        let entry = found.map(|e| ReportedAbsence {
            player_id,
            team_id,
            status,
            return_date,
            worth_elo: e.elo,
            se_elo: e.se_elo,
            missed_so_far,
        });
        let span = entry
            .as_ref()
            .and_then(|r| worth::span_for(r, report_date, season_end, rule.as_ref()));
        let applied = entry
            .as_ref()
            .map(|r| {
                worth::absences_from_report(
                    std::slice::from_ref(r),
                    report_date,
                    season_end,
                    rule.as_ref(),
                )
            })
            .unwrap_or_default();
        let one = applied.first();
        lines.push(CurrentAbsence {
            player_id,
            name,
            team_id,
            team_code,
            status,
            status_text,
            return_date,
            worth_elo: found.map(|e| e.elo),
            se_elo: found.map(|e| e.se_elo),
            worth_season: found.map(|e| e.season),
            worth_source: found.map(|e| e.source),
            penalty_elo: one.map(|a| a.elo),
            weight: one.and(span).map(|s| s.weight),
            weight_source: one.and(span).map(|s| s.weight_source),
            until: one.map(|a| a.to),
            span: one.and(span).map(|s| s.source),
            missed_so_far,
            lines_through: lines_through.get(&team_id).copied(),
            listed: is_listed,
        });
        if let Some(r) = entry {
            reported.push(r);
        }
    };
    for l in &report {
        listed.insert(l.player_id);
        let Some(team) = teams.get(&l.team_code) else {
            continue;
        };
        let status = AbsenceStatus::parse_line(&l.status, l.description.as_deref());
        price(
            l.player_id,
            l.player_name.clone(),
            team.id,
            l.team_code.clone(),
            status,
            l.status.clone(),
            l.return_date,
            true,
        );
    }
    // Regular players in a run not on the report: out, whatever the reason.
    let mut absent: Vec<&spell::CurrentRun> = current
        .current
        .iter()
        .filter(|c| {
            c.regular && c.missed >= worth::SPELL_MIN_MISSED && !listed.contains(&c.player_id)
        })
        .collect();
    absent.sort_by_key(|c| (c.team_id, c.player_id));
    let mut unlisted = 0u32;
    if !absent.is_empty() {
        let ids: Vec<PlayerId> = absent.iter().map(|c| c.player_id).collect();
        let players: HashMap<PlayerId, Player> = app
            .store
            .players_by_id(&ids)
            .await?
            .into_iter()
            .map(|p| (p.id, p))
            .collect();
        // A player who left the team is no absence: not on a roster after a refresh, or
        // listed with another team. Without any roster refresh the store cannot tell.
        let rosters_known = app.store.player_coverage().await?.active_players > 0;
        let codes: HashMap<TeamId, String> =
            teams.iter().map(|(code, t)| (t.id, code.clone())).collect();
        for c in absent {
            let player = players.get(&c.player_id);
            let left = player.is_some_and(|p| {
                (rosters_known && !p.active)
                    || p.team_code
                        .as_deref()
                        .is_some_and(|code| codes.get(&c.team_id).is_some_and(|own| own != code))
            });
            if left {
                continue;
            }
            unlisted += 1;
            price(
                c.player_id,
                player
                    .map(|p| {
                        format!("{} {}", p.first_name, p.last_name)
                            .trim()
                            .to_string()
                    })
                    .unwrap_or_else(|| c.player_id.to_string()),
                c.team_id,
                codes.get(&c.team_id).cloned().unwrap_or_default(),
                AbsenceStatus::Absent,
                format!(
                    "Absent from the box scores for {} games, not on the report",
                    c.missed
                ),
                None,
                false,
            );
        }
    }
    let absences = worth::absences_from_report(&reported, report_date, season_end, rule.as_ref());
    lines.sort_by(|a, b| {
        b.penalty_elo
            .unwrap_or(0.0)
            .total_cmp(&a.penalty_elo.unwrap_or(0.0))
            .then_with(|| a.team_code.cmp(&b.team_code))
            .then_with(|| a.name.cmp(&b.name))
    });
    let survival = fitted.map(|(s, seasons)| SurvivalSummary {
        model: s.model,
        seasons,
        runs: s.runs,
        censored: s.censored,
        longest: s.longest,
        mean_run_games: s.mean_run(),
        expected_remaining: s.curve(),
    });
    let statuses = measured.map(|(t, seasons)| StatusSummary {
        model: t.model,
        seasons,
        lines: t.lines,
        curves: t
            .curves
            .iter()
            .map(|c| StatusCurveSummary {
                status: c.status,
                in_run: c.in_run,
                samples: c.samples,
                weight: c.weight(),
                default_weight: c.status.weight(),
                expected_missed: c.expected_missed(),
                span_games: c.span_games(),
                default_days: c.status.default_days(),
                miss: c.miss.clone(),
            })
            .collect(),
    });
    Ok(Availability {
        season,
        report_date: Some(report_date),
        season_end,
        lines,
        absences,
        survival,
        statuses,
        unlisted,
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
            Some(Estimate {
                elo: 120.0,
                se_elo: 1.2 * 28.0,
                season: SeasonYear::of(2025),
                source: WorthSource::Worth,
            })
        );
        assert_eq!(pick_estimate(&tables, PlayerId::new(2)), None);
        assert_eq!(pick_estimate(&[], PlayerId::new(1)), None);
    }

    fn absence(player: Option<i32>, elo: f64, se: f64) -> Absence {
        let d = |day: u32| NaiveDate::from_ymd_opt(2026, 1, day).unwrap();
        Absence {
            team_id: TeamId::new(1),
            from: d(1),
            to: d(20),
            elo,
            player_id: player.map(PlayerId::new),
            se_elo: se,
        }
    }

    #[test]
    fn a_given_absence_replaces_the_players_report_line_and_the_rest_is_added() {
        let given = vec![absence(Some(1), 100.0, 40.0), absence(None, 30.0, 0.0)];
        let report = vec![absence(Some(1), 50.0, 10.0), absence(Some(2), 20.0, 5.0)];
        let mut merged = merge_with_report(given, report);
        assert_eq!(merged.len(), 3);
        assert_eq!(
            merged.iter().map(|a| a.elo).collect::<Vec<_>>(),
            vec![100.0, 30.0, 20.0]
        );
        fix_penalties(&mut merged);
        assert!(merged.iter().all(|a| a.se_elo == 0.0));
        assert_eq!(merged[0].elo, 100.0);
    }

    #[test]
    fn the_box_estimate_takes_the_longest_stint_of_the_latest_season() {
        use crate::domain::bpm::{BoxTable, Coverage, LeagueBox, PlayerBox};
        let row = |team: i32, minutes: f64, elo: f64| PlayerBox {
            player_id: PlayerId::new(1),
            team_id: TeamId::new(team),
            games: 10,
            minutes,
            minutes_share: 0.5,
            position: 3.0,
            offensive_role: 3.0,
            pts_per_100: 20.0,
            raw_bpm: 0.0,
            bpm: 0.0,
            obpm: 0.0,
            dbpm: 0.0,
            vorp: 0.0,
            wins: 0.0,
            per: None,
            worth_points: elo / 28.0,
            worth_elo: elo,
            turnovers_imputed: false,
        };
        let table = |season: i32, players: Vec<PlayerBox>| {
            (
                SeasonYear::of(season),
                Arc::new(BoxTable {
                    season: SeasonYear::of(season),
                    model: "test",
                    games: 1,
                    games_without_lines: 0,
                    teams_without_minutes: 0,
                    coverage: Coverage::default(),
                    bpm_available: true,
                    per_available: false,
                    per_partial: false,
                    league: LeagueBox::default(),
                    teams: vec![],
                    players,
                    note: None,
                }),
            )
        };
        let tables = vec![
            table(2026, vec![row(1, 300.0, 15.0), row(2, 900.0, 45.0)]),
            table(2025, vec![row(1, 2000.0, 80.0)]),
        ];
        let e = pick_box_estimate(&tables, PlayerId::new(1)).unwrap();
        assert_eq!(
            (e.elo, e.se_elo, e.season),
            (45.0, 0.0, SeasonYear::of(2026))
        );
        assert_eq!(e.source, WorthSource::Box);
        assert_eq!(pick_box_estimate(&tables, PlayerId::new(9)), None);
    }

    #[test]
    fn the_box_estimate_steps_in_when_the_worth_is_within_one_error_of_zero() {
        let est = |elo: f64, se: f64, source| Estimate {
            elo,
            se_elo: se,
            season: SeasonYear::of(2025),
            source,
        };
        let noisy = est(-16.0, 70.0, WorthSource::Worth);
        let clear = est(60.0, 20.0, WorthSource::Worth);
        let boxed = est(40.0, 0.0, WorthSource::Box);
        assert_eq!(choose_estimate(Some(noisy), Some(boxed)), Some(boxed));
        assert_eq!(choose_estimate(Some(clear), Some(boxed)), Some(clear));
        assert_eq!(choose_estimate(Some(noisy), None), Some(noisy));
        assert_eq!(choose_estimate(None, Some(boxed)), Some(boxed));
        assert_eq!(choose_estimate(None, None), None);
    }
}
