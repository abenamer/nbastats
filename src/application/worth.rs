//! Player worth per season and the availability of the current rosters, from the stored
//! games, box-score lines and injury reports; see `domain::worth`.

use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::sync::Arc;

use chrono::NaiveDate;
use serde::Serialize;

use crate::domain::bpm::BoxTable;
use crate::domain::full_strength;
use crate::domain::ids::{PlayerId, SeasonYear, TeamId};
use crate::domain::rookie::{self, DraftBracket};
use crate::domain::spell::{self, SeasonRuns, Survival};
use crate::domain::status::{self, StatusSample, StatusTable};
use crate::domain::worth::{
    self, Absence, AbsenceEffect, AbsenceStatus, BoxCalibration, NightGame, PlayerWorth,
    ReportedAbsence, SpanRule, SpanSource, WeightSource, WorthOverride, WorthParams, WorthTable,
};

use super::boxscore;
use super::ports::{GameLog, Player, PlayerStore, StoreError};
use super::rookie::{self as app_rookie, DraftSummary};
use super::state::{App, SeasonLoadError};
use crate::domain::models::{Game, GameType};

/// A report is priced with the estimate of the smallest error among the season's own table
/// and those of this many earlier seasons.
pub const WORTH_LOOKBACK: usize = 2;
/// A rookie's draft is at most this many years before the season priced; an older draft
/// marks a veteran whatever the stored box scores show.
pub const ROOKIE_DRAFT_YEARS: i32 = 5;
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
    /// The first season the estimate draws on; `worth_season` is the last.
    pub worth_first_season: Option<SeasonYear>,
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
    /// The bracket of the draft prior, when one priced the line (`worth_source` draft):
    /// the rookie's own bracket, or the wider one standing in for it.
    pub draft_bracket: Option<DraftBracket>,
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
    /// The draft priors the rookies' lines lean on; a rookie in a bracket without one (or a
    /// stand-in) keeps his box worth or stays without an estimate.
    pub draft: Option<DraftSummary>,
    /// How much of a box worth shows up as a team drop, measured over the players with
    /// both estimates in the seasons priced from; none below 30 such players, when the
    /// box worth stands unscaled and the older one-error rule chooses.
    pub box_calibration: Option<BoxCalibration>,
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

/// The Elo each side was missing in each played game of `season`
/// (`full_strength::game_absences`): the players the stored reports listed who then did
/// not play, priced as tonight's lines are, with the seasons' estimates. Cached until the
/// next import, results sync or player sync. Without stored reports, none.
pub async fn played_absences(
    app: &App,
    season: SeasonYear,
) -> Result<Arc<full_strength::MissingElo>, SeasonLoadError> {
    if let Some(r) = app
        .played_absences
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .get(&season)
        .cloned()
    {
        return Ok(r);
    }
    let generation = app.cache_generation.load(Ordering::SeqCst);
    let data = app.season_data(season).await?;
    let dates = data
        .games
        .iter()
        .filter(|g| g.is_played())
        .map(|g| g.game_date);
    let (Some(first), Some(last)) = (dates.clone().min(), dates.max()) else {
        return Ok(Arc::new(HashMap::new()));
    };
    let reports = app
        .store
        .injury_reports_between(
            first - chrono::Duration::days(full_strength::REPORT_MAX_AGE_DAYS),
            last,
        )
        .await?;
    let missing = if reports.is_empty() {
        Arc::new(HashMap::new())
    } else {
        let mut ids: Vec<PlayerId> = reports.iter().map(|r| r.player_id).collect();
        ids.sort_unstable();
        ids.dedup();
        let tables = lookback_tables(app, season).await?;
        let box_tables = box_lookback_tables(app, season).await?;
        let pricer = Pricer::load(app, season, tables, box_tables, &ids).await?;
        let worth_of: HashMap<PlayerId, (f64, f64)> = ids
            .iter()
            .filter_map(|id| {
                let (e, _) = pricer.estimate(*id, pricer.players.get(id));
                e.map(|e| (*id, (e.elo, e.se_elo)))
            })
            .collect();
        let lines = app.store.season_box_lines(season).await?;
        tokio::task::spawn_blocking(move || {
            Arc::new(full_strength::game_absences(
                &data.games,
                &lines,
                &reports,
                &worth_of,
            ))
        })
        .await
        .map_err(|e| SeasonLoadError::Store(StoreError::Query(e.to_string())))?
    };
    let mut cache = app
        .played_absences
        .write()
        .unwrap_or_else(|e| e.into_inner());
    if app.cache_generation.load(Ordering::SeqCst) == generation {
        cache.insert(season, missing.clone());
    }
    Ok(missing)
}

/// Every team's full-strength rating of `season` as of `as_of`: the played games replayed
/// with the Elo each side was missing in the expectation (`full_strength::replay`). The
/// gap to the stored rating is held within `full_strength::MAX_SHIFT_ELO`, the most a
/// forecast moves a rating, so the panel and a forecast read the same rating.
pub async fn full_strength(
    app: &App,
    season: SeasonYear,
    as_of: NaiveDate,
) -> Result<HashMap<TeamId, full_strength::FullStrength>, SeasonLoadError> {
    let data = app.season_data(season).await?;
    let missing = played_absences(app, season).await?;
    let mut rows = full_strength::replay(&data.games, &missing, as_of);
    for f in rows.values_mut() {
        let cap = full_strength::MAX_SHIFT_ELO;
        f.rating = f.stored + (f.rating - f.stored).clamp(-cap, cap);
    }
    Ok(rows)
}

/// The shifts that move each team from its stored rating to its full-strength one as of
/// `as_of`, for a forecast that prices the injury report; teams with none are left out.
/// Read at the forecast's own cutoff: never on or after the first playoff game, as the
/// engine moves a later date back to the day before the playoffs.
pub async fn rating_shifts(
    app: &App,
    season: SeasonYear,
    as_of: NaiveDate,
) -> Result<Vec<full_strength::RatingShift>, SeasonLoadError> {
    let data = app.season_data(season).await?;
    let first_playoff = data
        .games
        .iter()
        .filter(|g| g.is_playoff())
        .map(|g| g.game_date)
        .min();
    let cutoff = match first_playoff {
        Some(fp) if as_of >= fp => fp - chrono::Duration::days(1),
        _ => as_of,
    };
    let mut shifts: Vec<full_strength::RatingShift> = full_strength(app, season, cutoff)
        .await?
        .values()
        .filter(|f| (f.rating - f.stored).abs() > 1e-9)
        .map(|f| full_strength::RatingShift {
            team_id: f.team_id,
            // Clamped again: rebuilding the gap from the clamped rating can land a hair
            // past the cap, which the job's validation would refuse.
            elo: (f.rating - f.stored)
                .clamp(-full_strength::MAX_SHIFT_ELO, full_strength::MAX_SHIFT_ELO),
        })
        .collect();
    shifts.sort_by_key(|s| s.team_id);
    Ok(shifts)
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

/// What a season's report lines are priced against: the worth and box tables of the
/// season and the ones before, the box calibration, the draft priors, and for the players
/// asked about their records and first seasons (to tell a rookie). One per pricing, so the
/// availability answer and the full-strength replay price a player alike.
pub struct Pricer {
    season: SeasonYear,
    tables: Vec<(SeasonYear, Arc<WorthTable>)>,
    box_tables: Vec<(SeasonYear, Arc<BoxTable>)>,
    pub calibration: Option<BoxCalibration>,
    pub draft: Arc<app_rookie::DraftSummary>,
    pub players: HashMap<PlayerId, Player>,
    first_seasons: HashMap<PlayerId, SeasonYear>,
}

impl Pricer {
    /// The pricing of `season` for the players `ids`, over the tables given.
    pub async fn load(
        app: &App,
        season: SeasonYear,
        tables: Vec<(SeasonYear, Arc<WorthTable>)>,
        box_tables: Vec<(SeasonYear, Arc<BoxTable>)>,
        ids: &[PlayerId],
    ) -> Result<Self, SeasonLoadError> {
        let draft = app_rookie::draft_table(app, season).await?;
        let players: HashMap<PlayerId, Player> = if ids.is_empty() {
            HashMap::new()
        } else {
            app.store
                .players_by_id(ids)
                .await?
                .into_iter()
                .map(|p| (p.id, p))
                .collect()
        };
        // The first season each player has a line in: seen before this one, he is no
        // rookie whatever the draft fields say.
        let first_seasons = app.store.first_appearance_seasons(ids).await?;
        // How much of a box worth shows up as a team drop, over these seasons' players.
        let calibration = calibration_from(&tables, &box_tables);
        Ok(Self {
            season,
            tables,
            box_tables,
            calibration,
            draft,
            players,
            first_seasons,
        })
    }

    /// A player's box worth and minutes this season, over his stints.
    fn this_season(&self, player: PlayerId) -> Option<(f64, f64)> {
        self.box_tables
            .iter()
            .find(|(s, _)| *s == self.season)
            .and_then(|(_, t)| app_rookie::season_box(t, player))
    }

    /// Whether a player is new to the league: no line in any stored season before this
    /// one, whatever his draft year, so a first-year player, one who lost his first season,
    /// or an arrival from abroad. A draft older than five years before the season is no
    /// new player's: a gap in the stored box scores must not turn a veteran into one.
    /// A player without a record has no draft to bar him; his lines decide.
    pub fn is_new(&self, player_id: PlayerId, player: Option<&Player>) -> bool {
        !self
            .first_seasons
            .get(&player_id)
            .is_some_and(|s| *s < self.season)
            && player
                .and_then(|p| p.draft_year)
                .is_none_or(|y| y >= i32::from(self.season) - ROOKIE_DRAFT_YEARS)
    }

    /// The estimate a player is priced with, and the bracket of the prior when one priced
    /// it. `player` is his record, when known, to tell a rookie.
    pub fn estimate(
        &self,
        player_id: PlayerId,
        player: Option<&Player>,
    ) -> (Option<Estimate>, Option<DraftBracket>) {
        let season = self.season;
        let calibration = self.calibration.as_ref();
        let found = choose_estimate(
            pick_estimate(&self.tables, player_id),
            pick_box_estimate(&self.box_tables, player_id),
            calibration,
        );
        let Some(p) = player else {
            return (found, None);
        };
        if !self.is_new(player_id, Some(p)) {
            return (found, None);
        }
        let Some(prior) = self
            .draft
            .prior_for(DraftBracket::of(p.draft_round, p.draft_number))
        else {
            return (found, None);
        };
        match found {
            Some(e) if matches!(e.source, WorthSource::Worth | WorthSource::Blend) => {
                (Some(e), None)
            }
            other => {
                // The prior and his own box worth on the veterans' scale: through the
                // calibration when there is one.
                let scale = calibration.map_or(1.0, |c| c.slope);
                let scaled = rookie::DraftPrior {
                    bracket: prior.bracket,
                    players: prior.players,
                    mean_elo: prior.mean_elo * scale,
                    se_elo: prior.se_elo * scale,
                    sd_elo: prior.sd_elo * scale,
                };
                let own = other
                    .and_then(|_| self.this_season(player_id))
                    .map(|(worth, minutes)| (worth * scale, minutes));
                let (elo, se_elo) = rookie::shrink(&scaled, own);
                // Within one error of zero, the prior says nothing: the box worth stands,
                // or nothing does, as for a worth from the missed games.
                if elo.abs() < se_elo {
                    return (other, None);
                }
                (
                    Some(Estimate {
                        elo,
                        se_elo,
                        season,
                        first_season: season,
                        source: WorthSource::Draft,
                    }),
                    Some(prior.bracket),
                )
            }
        }
    }
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

/// The estimate to price a player with from the games his team played without him, over
/// the tables given (latest season first): the seasons pooled under one prior, that of the
/// latest season that has him (`worth::pool_seasons`), so a few games this season do not stand for a player
/// measured over the seasons before. None when no table has him.
pub fn pick_estimate(
    tables: &[(SeasonYear, Arc<WorthTable>)],
    player: PlayerId,
) -> Option<Estimate> {
    let found: Vec<(SeasonYear, worth::SeasonWorth)> = tables
        .iter()
        .filter_map(|(season, t)| {
            let w = t.players.iter().find(|w| w.player_id == player)?;
            Some((
                *season,
                worth::SeasonWorth {
                    elo: w.worth_elo,
                    se_elo: w.se_elo,
                    prior_sd_elo: t.prior_sd_points * worth::ELO_PER_POINT,
                },
            ))
        })
        .collect();
    let season = found.iter().map(|(s, _)| *s).max()?;
    let first_season = found.iter().map(|(s, _)| *s).min()?;
    let prior = tables
        .iter()
        .find(|(s, _)| *s == season)
        .map_or(0.0, |(_, t)| t.prior_sd_points * worth::ELO_PER_POINT);
    let seasons: Vec<worth::SeasonWorth> = found.iter().map(|(_, w)| *w).collect();
    let (elo, se_elo) = worth::pool_seasons(&seasons, prior);
    Some(Estimate {
        elo,
        se_elo,
        season,
        first_season,
        source: WorthSource::Worth,
    })
}

/// The worth estimate a report line is priced with, and the seasons it draws on: from
/// `first_season` to `season`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Estimate {
    pub elo: f64,
    pub se_elo: f64,
    pub season: SeasonYear,
    pub first_season: SeasonYear,
    pub source: WorthSource,
}

/// Where a worth estimate comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WorthSource {
    /// The regression on the games the team played without the player.
    Worth,
    /// Box Plus/Minus over replacement, per game played, scaled by the measured share of
    /// a box worth that shows up as a team drop, with that measurement's error; unscaled
    /// and without an error when no calibration exists.
    Box,
    /// The scaled box worth and the worth from the missed games, weighted by precision.
    Blend,
    /// A rookie's draft prior: what his draft slot's rookies have been worth, pulled
    /// toward his own box worth by his minutes; the error is the bracket's spread, shrunk
    /// the same way.
    Draft,
}

/// The box-score estimate for a player among the tables given (latest season first): his
/// box worth in each season over his stints, weighted by minutes, and the seasons pooled by
/// minutes (`worth::pool_box`), so a handful of games this season count for their minutes.
pub fn pick_box_estimate(
    tables: &[(SeasonYear, Arc<BoxTable>)],
    player: PlayerId,
) -> Option<Estimate> {
    let found: Vec<(SeasonYear, f64, f64)> = tables
        .iter()
        .filter_map(|(season, t)| {
            super::rookie::season_box(t, player).map(|(elo, minutes)| (*season, elo, minutes))
        })
        .collect();
    let pairs: Vec<(f64, f64)> = found.iter().map(|(_, e, m)| (*e, *m)).collect();
    let elo = worth::pool_box(&pairs)?;
    Some(Estimate {
        elo,
        se_elo: 0.0,
        season: found.iter().map(|(s, _, _)| *s).max()?,
        first_season: found.iter().map(|(s, _, _)| *s).min()?,
        source: WorthSource::Box,
    })
}

/// The estimate to price a player with. With a calibration, the box worth is scaled by
/// the measured share that shows up as a team drop and carries the calibration's error;
/// the player's own worth from the missed games, when he has one, is combined with it by
/// precision (`Blend`), so a noisy with/without season pulls a clear box score only as far
/// as its precision allows. Without a calibration the older rule stands: the worth when
/// it is at least one standard error from zero, else the box worth, else whichever exists.
pub fn choose_estimate(
    worth: Option<Estimate>,
    boxed: Option<Estimate>,
    calibration: Option<&BoxCalibration>,
) -> Option<Estimate> {
    let Some(c) = calibration else {
        return match (worth, boxed) {
            (Some(w), Some(b)) if w.elo.abs() < w.se_elo => Some(b),
            (Some(w), _) => Some(w),
            (None, b) => b,
        };
    };
    let scaled = boxed.map(|b| {
        let (elo, se_elo) = c.prior(b.elo);
        Estimate {
            elo,
            se_elo,
            season: b.season,
            first_season: b.first_season,
            source: WorthSource::Box,
        }
    });
    match (worth, scaled) {
        (Some(w), Some(b)) => {
            let (elo, se_elo) = worth::combine((b.elo, b.se_elo), (w.elo, w.se_elo));
            Some(Estimate {
                elo,
                se_elo,
                season: w.season.max(b.season),
                first_season: w.first_season.min(b.first_season),
                source: WorthSource::Blend,
            })
        }
        (Some(w), None) => Some(w),
        (None, b) => b,
    }
}

/// The calibration of the box worth from the players with both estimates in the same
/// season, over the tables given.
pub fn calibration_from(
    tables: &[(SeasonYear, Arc<WorthTable>)],
    box_tables: &[(SeasonYear, Arc<BoxTable>)],
) -> Option<BoxCalibration> {
    let mut pairs = Vec::new();
    for (season, t) in tables {
        let Some((_, b)) = box_tables.iter().find(|(s, _)| s == season) else {
            continue;
        };
        for w in &t.players {
            if w.se_elo <= 0.0 {
                continue;
            }
            if let Some((box_elo, _)) = super::rookie::season_box(b, w.player_id) {
                pairs.push((w.worth_elo, w.se_elo, box_elo));
            }
        }
    }
    worth::calibrate_box(&pairs)
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
        draft: None,
        box_calibration: None,
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
    let listed_ids: Vec<PlayerId> = report.iter().map(|l| l.player_id).collect();
    let pricer = Pricer::load(app, season, tables, box_tables, &listed_ids).await?;
    let listed_players = &pricer.players;
    let estimate =
        |player_id: PlayerId, player: Option<&Player>| pricer.estimate(player_id, player);
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
                     is_listed: bool,
                     player: Option<&Player>| {
        let missed_so_far = lines_through.get(&team_id).map(|_| {
            in_progress
                .get(&(team_id, player_id))
                .map_or(0, |c| c.missed + since_lines(team_id))
        });
        let (found, draft_bracket) = estimate(player_id, player);
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
            worth_first_season: found.map(|e| e.first_season),
            worth_source: found.map(|e| e.source),
            penalty_elo: one.map(|a| a.elo),
            weight: one.and(span).map(|s| s.weight),
            weight_source: one.and(span).map(|s| s.weight_source),
            until: one.map(|a| a.to),
            span: one.and(span).map(|s| s.source),
            missed_so_far,
            lines_through: lines_through.get(&team_id).copied(),
            listed: is_listed,
            draft_bracket,
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
            listed_players.get(&l.player_id),
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
                player,
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
        draft: Some((*pricer.draft).clone()),
        box_calibration: pricer.calibration.clone(),
        unlisted,
        note: None,
    })
}

/// A team's current rating: the post-game rating of its last played game, else the
/// pre-game rating of its first scheduled game (preseason). The date is the game's, and the
/// flag says whether a game was played.
pub fn rating_now(games: &[Game], team: TeamId) -> Option<(f64, NaiveDate, bool)> {
    rating_as_of(games, team, NaiveDate::MAX)
}

/// The same, read as of a date: the last played game dated `as_of` or before.
pub fn rating_as_of(
    games: &[Game],
    team: TeamId,
    as_of: NaiveDate,
) -> Option<(f64, NaiveDate, bool)> {
    let last_played = games
        .iter()
        .filter(|g| g.is_played() && g.involves(team) && g.game_date <= as_of)
        .max_by_key(|g| (g.game_date, g.seq));
    if let Some(g) = last_played {
        let elo = if g.home_team_id == team {
            g.home_elo_post
        } else {
            g.away_elo_post
        };
        return Some((elo, g.game_date, true));
    }
    let first = games
        .iter()
        .filter(|g| g.involves(team))
        .min_by_key(|g| (g.game_date, g.seq))?;
    let elo = if first.home_team_id == team {
        first.home_elo_pre
    } else {
        first.away_elo_pre
    };
    Some((elo, first.game_date, false))
}

/// One line of a team's injury card: the priced line and what it costs over the games it
/// covers.
#[derive(Debug, Clone, Serialize)]
pub struct CardLine {
    #[serde(flatten)]
    pub line: CurrentAbsence,
    /// The penalty that prices the games: the fixed penalty when the worth has no error,
    /// else the mean of the draw held between zero and the cap (see
    /// `domain::worth::expected_penalty`), which the forecast applies on average. None
    /// when nothing is priced.
    pub expected_penalty_elo: Option<f64>,
    /// Remaining games the penalty covers.
    pub games_covered: u32,
    /// The penalty summed over them, in Elo times games.
    pub elo_games: f64,
    /// Its part of the team's mean penalty over the remaining games.
    pub mean_penalty_elo: f64,
    /// Expected wins lost to this line alone, to first order.
    pub wins_lost_alone: f64,
}

/// A team's injury card: every report line priced for it, what each costs over the games
/// it covers, and the sum two ways: as listed, and with today's absences held to the
/// season's end.
#[derive(Debug, Clone, Serialize)]
pub struct InjuryCard {
    pub season: SeasonYear,
    pub team_id: TeamId,
    pub team_code: String,
    pub team_name: String,
    pub report_date: Option<NaiveDate>,
    /// The date the card is read as of: today, or the report date for a season already
    /// over, so a stored report of a past season still reads.
    pub today: NaiveDate,
    pub season_end: NaiveDate,
    /// The team's full-strength rating as of that date (`full_strength`), the date of its
    /// last game, and whether no game is played yet.
    pub rating_elo: Option<f64>,
    /// The stored Elo at the same point, which the absences of the games played are in.
    pub rating_stored: Option<f64>,
    /// The team's games played short, whose absences the full-strength rating takes out.
    pub games_short: u32,
    pub rating_date: Option<NaiveDate>,
    pub rating_is_preseason: Option<bool>,
    /// The regular-season home bonus the win probabilities use: the one asked for, else
    /// the season's fitted one.
    pub home_advantage: f64,
    /// The season's fitted bonus, so a reader can tell whether `home_advantage` is it.
    pub home_advantage_fitted: f64,
    /// The penalty on today's date, and the rating the team plays at today.
    pub penalty_today: f64,
    pub rating_today: Option<f64>,
    pub lines: Vec<CardLine>,
    /// The date of the last regular-season game, which the held total runs to.
    pub last_regular_game: NaiveDate,
    /// The absences as listed, each to its own end.
    pub listed: AbsenceEffect,
    /// Today's absences held to the last regular-season game.
    pub held: AbsenceEffect,
    /// The season's regular-season home bonus and the survival and status curves are
    /// those of the availability answer; its note, when nothing is priced.
    pub note: Option<String>,
    /// The reader's what-ifs the card is priced with, resolved.
    pub what_ifs: Vec<WhatIf>,
}

/// The injury card of `code` in `season`; none for a team not in the season.
pub async fn injury_card(
    app: &App,
    season: SeasonYear,
    code: &str,
    home_advantage: Option<f64>,
    what_ifs: &[WhatIf],
) -> Result<Option<InjuryCard>, SeasonLoadError> {
    let data = app.season_data(season).await?;
    let teams = app.store.team_code_map().await?;
    let code = code.trim().to_ascii_uppercase();
    let Some(team) = teams.get(&code) else {
        return Ok(None);
    };
    if !data.games.iter().any(|g| g.involves(team.id)) {
        return Ok(None);
    }
    let mut a = availability(app, season).await?;
    let shifts = what_if_shifts(what_ifs);
    worth::override_absences(&mut a.absences, &resolved_worths(what_ifs));
    let today = match a.report_date {
        Some(report) if app.today() > a.season_end => report,
        _ => app.today(),
    };
    let home_advantage_fitted = app.home_advantage(season).await?.regular;
    let home_advantage = home_advantage.unwrap_or(home_advantage_fitted);
    // The full-strength ratings: the absences of the games played are out of them, so
    // today's absences come off the full team once.
    let full = full_strength(app, season, today).await?;
    let ratings: HashMap<TeamId, f64> = full
        .iter()
        .map(|(t, f)| (*t, f.rating + shifts.get(t).copied().unwrap_or(0.0)))
        .collect();
    let rating = rating_as_of(&data.games, team.id, today);
    let own_shift = shifts.get(&team.id).copied().unwrap_or(0.0);
    let own = full.get(&team.id).map(|f| full_strength::FullStrength {
        rating: f.rating + own_shift,
        ..*f
    });
    let absences: Vec<Absence> = a
        .absences
        .iter()
        .filter(|x| x.team_id == team.id)
        .cloned()
        .collect();
    let effect = |abs: &[Absence]| {
        worth::absence_effect(abs, &data.games, &ratings, team.id, today, home_advantage)
    };
    let last_regular_game = data
        .games
        .iter()
        .filter(|g| g.game_type == GameType::Regular)
        .map(|g| g.game_date)
        .max()
        .unwrap_or(a.season_end);
    let listed = effect(&absences);
    let held = effect(&worth::held(&absences, today, last_regular_game));
    let penalty_today = worth::penalty(&absences, team.id, today);
    let lines = a
        .lines
        .iter()
        .filter(|l| l.team_id == team.id)
        .map(|l| {
            let own: Vec<Absence> = absences
                .iter()
                .filter(|x| x.player_id == Some(l.player_id))
                .cloned()
                .collect();
            let e = effect(&own);
            CardLine {
                line: l.clone(),
                expected_penalty_elo: own
                    .first()
                    .map(|x| worth::expected_penalty(x.elo, x.se_elo)),
                games_covered: e.games_covered,
                elo_games: e.elo_games,
                mean_penalty_elo: e.mean_penalty,
                wins_lost_alone: e.wins_lost,
            }
        })
        .collect();
    Ok(Some(InjuryCard {
        season,
        team_id: team.id,
        team_code: team.code.clone(),
        team_name: team.name.clone(),
        report_date: a.report_date,
        today,
        season_end: a.season_end,
        rating_elo: own.map(|f| f.rating),
        rating_stored: rating.map(|(e, _, _)| e),
        games_short: own.map_or(0, |f| f.games_short),
        rating_date: rating.map(|(_, d, _)| d),
        rating_is_preseason: rating.map(|(_, _, played)| !played),
        home_advantage,
        home_advantage_fitted,
        penalty_today,
        rating_today: own.map(|f| f.rating - penalty_today),
        lines,
        last_regular_game,
        listed,
        held,
        note: a.note,
        what_ifs: what_ifs.to_vec(),
    }))
}

/// A reader's what-if on a player's worth, resolved: the model's own estimate of him, his
/// current team, and the shift the team's rating takes (the what-if less the estimate, or
/// the whole what-if for a player without one).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct WhatIf {
    pub player_id: PlayerId,
    pub name: String,
    pub team_id: TeamId,
    pub team_code: String,
    /// The what-if worth: the one typed, or the one read from the window of games.
    pub elo: f64,
    /// The window the worth was read from, and how: "last 5 games: BPM +7.2 in 34 min a
    /// game, 61 Elo on the box scale, 44 on the worth scale"; none for a typed worth.
    pub last_games: Option<u8>,
    pub basis: Option<String>,
    pub current_elo: Option<f64>,
    pub current_source: Option<WorthSource>,
    pub shift_elo: f64,
}

/// Why a what-if cannot be priced.
#[derive(Debug, thiserror::Error)]
pub enum WhatIfError {
    #[error("no player {0} is stored")]
    UnknownPlayer(PlayerId),
    #[error("{0} has no current team on the last roster refresh, so his what-if moves no rating")]
    NoTeam(String),
    #[error("{0} has no played regular-season game with a full box score this season to read a worth from")]
    NoWindow(String),
    #[error("the what-if for player {0} needs one basis: a worth in Elo or a window of games")]
    Invalid(PlayerId),
    #[error(transparent)]
    Load(#[from] SeasonLoadError),
}

/// A player's worth read from a window of his games: the box-score worth of those games
/// alone, as if the season were only them, scaled to the worth scale by the season's box
/// calibration when there is one.
#[derive(Debug, Clone, PartialEq)]
pub struct WindowWorth {
    pub games: u8,
    pub minutes_per_game: f64,
    pub bpm: f64,
    pub box_elo: f64,
    pub elo: f64,
}

/// The game identifiers of a player's last `n` played regular-season games, in date order,
/// from his lines of the season: fewer than `n` when he has fewer.
pub fn window_game_ids(played: &[super::ports::Appearance], n: u8) -> Vec<i64> {
    let mut played: Vec<&super::ports::Appearance> = played
        .iter()
        .filter(|a| a.game_type == GameType::Regular && a.seconds.is_some_and(|s| s > 0))
        .collect();
    played.sort_by_key(|a| (a.game_date, a.game_id));
    let keep = played.len().saturating_sub(usize::from(n));
    played[keep..].iter().map(|a| a.game_id).collect()
}

/// A player's worth read from a box-score table built from a window of his games: his
/// box worth pooled over his stints by minutes, scaled by the calibration; the BPM is
/// minutes-weighted. None without BPM or without minutes for him.
pub fn window_from_table(
    table: &crate::domain::bpm::BoxTable,
    player: PlayerId,
    calibration: Option<&BoxCalibration>,
) -> Option<WindowWorth> {
    if !table.bpm_available {
        return None;
    }
    let (box_elo, minutes) = app_rookie::season_box(table, player)?;
    let rows: Vec<_> = table
        .players
        .iter()
        .filter(|p| p.player_id == player)
        .collect();
    let games_played: u32 = rows.iter().map(|p| p.games).sum();
    let bpm = rows.iter().map(|p| p.bpm * p.minutes).sum::<f64>() / minutes;
    Some(WindowWorth {
        games: u8::try_from(games_played).unwrap_or(u8::MAX),
        minutes_per_game: minutes / f64::from(games_played.max(1)),
        bpm,
        box_elo,
        elo: calibration.map_or(box_elo, |c| c.prior(box_elo).0),
    })
}

/// The window worths of the window what-ifs, in one read of the lines of every window's
/// games and one table build per player off the request thread. A player without a
/// window has none.
async fn window_worths(
    app: &App,
    season: SeasonYear,
    windows: &[(PlayerId, u8)],
    calibration: Option<&BoxCalibration>,
) -> Result<HashMap<PlayerId, Option<WindowWorth>>, SeasonLoadError> {
    let mut out = HashMap::new();
    if windows.is_empty() {
        return Ok(out);
    }
    let data = app.season_data(season).await?;
    let mut per_player: Vec<(PlayerId, Vec<i64>)> = Vec::with_capacity(windows.len());
    for (player, n) in windows {
        let played = app.store.player_appearances(*player, season).await?;
        per_player.push((*player, window_game_ids(&played, *n)));
    }
    let mut all_ids: Vec<i64> = per_player
        .iter()
        .flat_map(|(_, ids)| ids.iter().copied())
        .collect();
    all_ids.sort_unstable();
    all_ids.dedup();
    let lines = app.store.box_lines_of_games(&all_ids).await?;
    let mut player_ids: Vec<PlayerId> = lines.iter().map(|l| l.player_id).collect();
    player_ids.sort_unstable();
    player_ids.dedup();
    let listed: HashMap<PlayerId, f64> = app
        .store
        .players_by_id(&player_ids)
        .await?
        .into_iter()
        .filter_map(|p| {
            p.position
                .as_deref()
                .and_then(crate::domain::bpm::listed_position)
                .map(|v| (p.id, v))
        })
        .collect();
    let calibration = calibration.cloned();
    // A table per window, each a fixed-point iteration over a few hundred lines: off the
    // request thread, as the season's table is.
    let built = tokio::task::spawn_blocking(move || {
        per_player
            .into_iter()
            .map(|(player, ids)| {
                let games: Vec<Game> = data
                    .games
                    .iter()
                    .filter(|g| ids.contains(&g.id))
                    .cloned()
                    .collect();
                let own: Vec<_> = lines
                    .iter()
                    .filter(|l| ids.contains(&l.game_id))
                    .copied()
                    .collect();
                let table = crate::domain::bpm::table(season, &games, &own, &listed);
                (
                    player,
                    window_from_table(&table, player, calibration.as_ref()),
                )
            })
            .collect::<Vec<_>>()
    })
    .await
    .map_err(|e| SeasonLoadError::Store(StoreError::Query(e.to_string())))?;
    out.extend(built);
    Ok(out)
}

/// Resolve the reader's what-ifs for `season`: each player's estimate, team and shift.
pub async fn what_ifs(
    app: &App,
    season: SeasonYear,
    overrides: &[WorthOverride],
) -> Result<Vec<WhatIf>, WhatIfError> {
    if overrides.is_empty() {
        return Ok(Vec::new());
    }
    let ids: Vec<PlayerId> = overrides.iter().map(|o| o.player_id).collect();
    let tables = lookback_tables(app, season).await?;
    let box_tables = box_lookback_tables(app, season).await?;
    let pricer = Pricer::load(app, season, tables, box_tables, &ids).await?;
    let teams = app
        .store
        .team_code_map()
        .await
        .map_err(SeasonLoadError::from)?;
    let windows: Vec<(PlayerId, u8)> = overrides
        .iter()
        .filter_map(|o| o.last_games.map(|n| (o.player_id, n)))
        .collect();
    let windows = window_worths(app, season, &windows, pricer.calibration.as_ref()).await?;
    let mut out = Vec::with_capacity(overrides.len());
    for o in overrides {
        let Some(player) = pricer.players.get(&o.player_id) else {
            return Err(WhatIfError::UnknownPlayer(o.player_id));
        };
        let name = format!("{} {}", player.first_name, player.last_name)
            .trim()
            .to_string();
        let Some(team) = player
            .team_code
            .as_deref()
            .and_then(|c| teams.get(&c.trim().to_ascii_uppercase()))
        else {
            return Err(WhatIfError::NoTeam(name));
        };
        let (estimate, _) = pricer.estimate(o.player_id, Some(player));
        let current = estimate.map(|e| e.elo);
        let (elo, last_games, basis) = match (o.elo, o.last_games) {
            (Some(elo), None) => (elo, None, None),
            (None, Some(_)) => {
                let Some(w) = windows.get(&o.player_id).and_then(|w| w.as_ref()) else {
                    return Err(WhatIfError::NoWindow(name));
                };
                let basis = format!(
                    "last {} game{}: BPM {:+.1} in {:.0} min a game, {:.0} Elo on the box scale, {:.0} on the worth scale",
                    w.games,
                    if w.games == 1 { "" } else { "s" },
                    w.bpm,
                    w.minutes_per_game,
                    w.box_elo,
                    w.elo
                );
                (w.elo, Some(w.games), Some(basis))
            }
            _ => return Err(WhatIfError::Invalid(o.player_id)),
        };
        let elo = elo.clamp(-worth::MAX_ABSENCE_ELO, worth::MAX_ABSENCE_ELO);
        out.push(WhatIf {
            player_id: o.player_id,
            name,
            team_id: team.id,
            team_code: team.code.clone(),
            elo,
            last_games,
            basis,
            current_elo: current,
            current_source: estimate.map(|e| e.source),
            shift_elo: (elo - current.unwrap_or(0.0))
                .clamp(-full_strength::MAX_SHIFT_ELO, full_strength::MAX_SHIFT_ELO),
        });
    }
    Ok(out)
}

/// The what-ifs' resolved worths, for the absence pricing.
pub fn resolved_worths(what_ifs: &[WhatIf]) -> Vec<(PlayerId, f64)> {
    what_ifs.iter().map(|w| (w.player_id, w.elo)).collect()
}

/// The what-ifs' shifts summed per team, each team's sum held within the shift cap.
pub fn what_if_shifts(what_ifs: &[WhatIf]) -> HashMap<TeamId, f64> {
    let mut shifts: HashMap<TeamId, f64> = HashMap::new();
    for w in what_ifs {
        let s = shifts.entry(w.team_id).or_insert(0.0);
        *s = (*s + w.shift_elo).clamp(-full_strength::MAX_SHIFT_ELO, full_strength::MAX_SHIFT_ELO);
    }
    shifts
}

/// A player out on the night, for the Tonight panel: one absence that covers it.
#[derive(Debug, Clone, Serialize)]
pub struct OutTonight {
    pub player_id: Option<PlayerId>,
    pub name: String,
    pub status: Option<AbsenceStatus>,
    pub status_text: String,
    /// The penalty the night carries for him: the fixed one, or with an error on the
    /// worth the mean of the draw held between zero and the cap. The team's penalty is
    /// the sum of these.
    pub penalty_elo: f64,
    /// Worth times weight, before the error is taken into account.
    pub fixed_penalty_elo: f64,
    pub until: NaiveDate,
}

/// One team on the night: its rating, what it plays without, and whom it plays.
#[derive(Debug, Clone, Serialize)]
pub struct TeamTonight {
    pub team_id: TeamId,
    pub code: String,
    pub name: String,
    /// The full-strength rating as of the date (`full_strength`); the league mean for a
    /// team without one.
    pub rating: f64,
    /// The stored Elo at the same point, which the absences of the games played are in.
    pub stored_rating: f64,
    /// The team's games played short, whose absences the full-strength rating takes out.
    pub games_short: u32,
    /// The sum of the `out` penalties.
    pub penalty: f64,
    pub rating_tonight: f64,
    pub out: Vec<OutTonight>,
    /// The opponent's code when the team plays on the night.
    pub opponent: Option<String>,
    pub home: Option<bool>,
}

/// A game of the night with the codes beside the identifiers.
#[derive(Debug, Clone, Serialize)]
pub struct GameTonight {
    #[serde(flatten)]
    pub game: NightGame,
    pub home_code: String,
    pub away_code: String,
}

/// The next night with regular-season games: every team's rating tonight and the games
/// priced at those ratings.
#[derive(Debug, Clone, Serialize)]
pub struct Tonight {
    pub season: SeasonYear,
    /// The date the panel is read as of (see [`InjuryCard::today`]).
    pub as_of: NaiveDate,
    /// The night: the earliest date on or after `as_of` with an unplayed regular-season
    /// game; none when the regular season is over.
    pub night: Option<NaiveDate>,
    /// The night's regular-season games already played, when it is partly over.
    pub games_played: u32,
    pub report_date: Option<NaiveDate>,
    /// The team games of the season played short, whose absences the full-strength ratings
    /// take out, and the model that replays them.
    pub games_played_short: u32,
    pub rating_model: &'static str,
    /// The regular-season home bonus the night is priced with: the one asked for, else
    /// the season's fitted one, which `home_advantage_fitted` gives either way.
    pub home_advantage: f64,
    pub home_advantage_fitted: f64,
    pub games: Vec<GameTonight>,
    /// Every team of the season, by rating tonight, highest first; empty when no night
    /// remains.
    pub teams: Vec<TeamTonight>,
    pub note: Option<String>,
    /// The reader's what-ifs the night is priced with, resolved.
    pub what_ifs: Vec<WhatIf>,
    /// The absences the night was priced from, so a reader that needs more from them (the
    /// mean over the remaining games) reads the same report.
    #[serde(skip)]
    pub absences: Vec<worth::Absence>,
}

/// Tonight's slate and every team's rating tonight for `season`.
pub async fn tonight(
    app: &App,
    season: SeasonYear,
    home_advantage: Option<f64>,
    what_ifs: &[WhatIf],
) -> Result<Tonight, SeasonLoadError> {
    let data = app.season_data(season).await?;
    let teams = app.store.team_code_map().await?;
    let mut a = availability(app, season).await?;
    // The what-ifs, resolved by `what_ifs`: each player's team moves by the difference,
    // and his absences are priced at the what-if.
    let shifts = what_if_shifts(what_ifs);
    worth::override_absences(&mut a.absences, &resolved_worths(what_ifs));
    let as_of = match a.report_date {
        Some(report) if app.today() > a.season_end => report,
        _ => app.today(),
    };
    let home_advantage_fitted = app.home_advantage(season).await?.regular;
    let home_advantage = home_advantage.unwrap_or(home_advantage_fitted);
    // The full-strength ratings, so tonight's absences come off the full team once.
    let full = full_strength(app, season, as_of).await?;
    let ratings: HashMap<TeamId, f64> = full
        .iter()
        .map(|(t, f)| (*t, f.rating + shifts.get(t).copied().unwrap_or(0.0)))
        .collect();
    let games_short: u32 = full.values().map(|f| f.games_short).sum();
    let slate = worth::tonight(&data.games, as_of, &ratings, &a.absences, home_advantage);
    let night = slate.as_ref().map(|n| n.date);
    let games_played = slate.as_ref().map_or(0, |n| n.played);
    let by_id: HashMap<TeamId, (&String, &str)> = teams
        .iter()
        .map(|(code, t)| (t.id, (code, t.name.as_str())))
        .collect();
    let code_of = |t: TeamId| {
        by_id
            .get(&t)
            .map_or_else(|| t.to_string(), |(c, _)| (*c).clone())
    };
    let games: Vec<GameTonight> = slate
        .as_ref()
        .map(|n| {
            n.games
                .iter()
                .map(|game| GameTonight {
                    game: game.clone(),
                    home_code: code_of(game.home_team_id),
                    away_code: code_of(game.away_team_id),
                })
                .collect()
        })
        .unwrap_or_default();
    // Every team of the season's games; without a night there is nothing to price.
    let mut season_teams: Vec<TeamId> = data
        .games
        .iter()
        .flat_map(|g| [g.home_team_id, g.away_team_id])
        .collect();
    season_teams.sort();
    season_teams.dedup();
    let team_row = |team_id: TeamId, on_date: NaiveDate| {
        let rating = ratings
            .get(&team_id)
            .copied()
            .unwrap_or(crate::domain::elo::SEASON_MEAN);
        // The list and the sum come from the same absences, so they agree.
        let out: Vec<OutTonight> = a
            .absences
            .iter()
            .filter(|x| x.team_id == team_id && x.covers(on_date))
            .map(|x| {
                let line = x.player_id.and_then(|id| {
                    a.lines
                        .iter()
                        .find(|l| l.player_id == id && l.team_id == team_id)
                });
                OutTonight {
                    player_id: x.player_id,
                    name: line.map_or_else(
                        || {
                            x.player_id
                                .map_or_else(|| "marked out".to_string(), |id| id.to_string())
                        },
                        |l| l.name.clone(),
                    ),
                    status: line.map(|l| l.status),
                    status_text: line.map_or_else(|| "out".to_string(), |l| l.status_text.clone()),
                    penalty_elo: worth::expected_penalty(x.elo, x.se_elo),
                    fixed_penalty_elo: x.elo,
                    until: x.to,
                }
            })
            .collect();
        let penalty = out
            .iter()
            .map(|o| o.penalty_elo)
            .fold(0.0, |acc, p| acc + p);
        let game = games
            .iter()
            .find(|g| g.game.home_team_id == team_id || g.game.away_team_id == team_id);
        let (name, code) = by_id.get(&team_id).map_or_else(
            || (team_id.to_string(), team_id.to_string()),
            |(c, n)| (n.to_string(), (*c).clone()),
        );
        TeamTonight {
            team_id,
            code,
            name,
            rating,
            stored_rating: full.get(&team_id).map_or(rating, |f| f.stored),
            games_short: full.get(&team_id).map_or(0, |f| f.games_short),
            penalty,
            rating_tonight: rating - penalty,
            out,
            opponent: game.map(|g| {
                if g.game.home_team_id == team_id {
                    g.away_code.clone()
                } else {
                    g.home_code.clone()
                }
            }),
            home: game.map(|g| g.game.home_team_id == team_id),
        }
    };
    let mut rows: Vec<TeamTonight> = night
        .map(|on_date| {
            season_teams
                .iter()
                .map(|&team_id| team_row(team_id, on_date))
                .collect()
        })
        .unwrap_or_default();
    rows.sort_by(|x, y| {
        y.rating_tonight
            .total_cmp(&x.rating_tonight)
            .then_with(|| x.code.cmp(&y.code))
    });
    Ok(Tonight {
        season,
        as_of,
        night,
        games_played,
        report_date: a.report_date,
        games_played_short: games_short,
        rating_model: full_strength::FULL_STRENGTH_MODEL,
        home_advantage,
        home_advantage_fitted,
        games,
        teams: rows,
        note: a.note,
        what_ifs: what_ifs.to_vec(),
        absences: a.absences,
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
    fn the_seasons_without_the_player_are_pooled_under_the_latest_prior() {
        let tables = vec![
            table_with(2026, 3.5, 10.0),
            table_with(2025, 1.2, 120.0),
            table_with(2024, 1.2, 90.0),
        ];
        let e = pick_estimate(&tables, PlayerId::new(1)).unwrap();
        let prior = 3.0 * 28.0;
        let season = |se: f64, elo: f64| worth::SeasonWorth {
            elo,
            se_elo: se * 28.0,
            prior_sd_elo: prior,
        };
        let expected = worth::pool_seasons(
            &[season(3.5, 10.0), season(1.2, 120.0), season(1.2, 90.0)],
            prior,
        );
        assert_eq!((e.elo, e.se_elo), expected);
        assert_eq!(
            (e.season, e.first_season, e.source),
            (
                SeasonYear::of(2026),
                SeasonYear::of(2024),
                WorthSource::Worth
            )
        );
        // The five-game season at 10 barely moves the two measured ones, at 90 and 120;
        // with their games added up, the one prior pulls them less toward zero than each
        // season's did.
        assert!(e.elo > 100.0 && e.elo < 120.0, "{}", e.elo);
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
    fn a_window_takes_the_last_played_games_and_reads_the_table_by_minutes() {
        use crate::domain::bpm::{BoxTable, Coverage, LeagueBox, PlayerBox};
        let d = |day: u32| NaiveDate::from_ymd_opt(2026, 11, day).unwrap();
        let app = |id: i64, day: u32, seconds: Option<i32>, kind: GameType| {
            super::super::ports::Appearance {
                game_id: id,
                season: SeasonYear::of(2027),
                game_date: d(day),
                game_type: kind,
                player_id: PlayerId::new(1),
                team_id: TeamId::new(1),
                team_code: "T1".into(),
                opponent_code: "T2".into(),
                home: true,
                seconds,
                pts: None,
                reb: None,
                oreb: None,
                dreb: None,
                ast: None,
                stl: None,
                blk: None,
                tov: None,
                pf: None,
                fgm: None,
                fga: None,
                fg3m: None,
                fg3a: None,
                ftm: None,
                fta: None,
                plus_minus: None,
            }
        };
        // Five regular-season games played, one not played and a playoff game: the last
        // three played are 4, 6 and 7 by date, however the lines come.
        let played = vec![
            app(6, 6, Some(1_800), GameType::Regular),
            app(2, 2, Some(1_200), GameType::Regular),
            app(4, 4, Some(1_500), GameType::Regular),
            app(5, 5, Some(0), GameType::Regular),
            app(7, 7, Some(1_900), GameType::Regular),
            app(8, 8, Some(2_000), GameType::Playoff),
            app(1, 1, Some(900), GameType::Regular),
        ];
        assert_eq!(window_game_ids(&played, 3), vec![4, 6, 7]);
        assert_eq!(window_game_ids(&played, 10), vec![1, 2, 4, 6, 7]);
        assert!(window_game_ids(&[], 5).is_empty());
        // The table: two stints pooled by minutes; the calibration halves the box worth.
        let row = |team: i32, games: u32, minutes: f64, bpm: f64, worth: f64| PlayerBox {
            player_id: PlayerId::new(1),
            team_id: TeamId::new(team),
            games,
            minutes,
            minutes_share: 0.7,
            position: 3.0,
            offensive_role: 3.0,
            pts_per_100: 20.0,
            raw_bpm: bpm,
            bpm,
            obpm: bpm / 2.0,
            dbpm: bpm / 2.0,
            vorp: 1.0,
            wins: 2.7,
            per: None,
            worth_points: worth / 28.0,
            worth_elo: worth,
            turnovers_imputed: false,
        };
        let table = BoxTable {
            season: SeasonYear::of(2027),
            model: "test",
            games: 3,
            games_without_lines: 0,
            teams_without_minutes: 0,
            coverage: Coverage::default(),
            bpm_available: true,
            per_available: false,
            per_partial: false,
            league: LeagueBox::default(),
            teams: vec![],
            players: vec![row(1, 2, 60.0, 6.0, 60.0), row(2, 1, 30.0, 3.0, 30.0)],
            note: None,
        };
        let w = window_from_table(&table, PlayerId::new(1), None).unwrap();
        assert_eq!((w.games, w.minutes_per_game), (3, 30.0));
        assert!((w.bpm - 5.0).abs() < 1e-12 && (w.box_elo - 50.0).abs() < 1e-12);
        assert_eq!(w.elo, w.box_elo);
        let c = BoxCalibration {
            model: "test",
            pairs: 100,
            slope: 0.5,
            slope_se: 0.1,
            error_elo: 10.0,
        };
        let scaled = window_from_table(&table, PlayerId::new(1), Some(&c)).unwrap();
        assert!((scaled.elo - 25.0).abs() < 1e-12);
        // No BPM, or a player not in the table: nothing to read.
        assert!(window_from_table(&table, PlayerId::new(9), None).is_none());
        let bare = BoxTable {
            bpm_available: false,
            ..table.clone()
        };
        assert!(window_from_table(&bare, PlayerId::new(1), None).is_none());
    }

    #[test]
    fn what_ifs_sum_per_team_within_the_cap_and_turn_back_into_overrides() {
        let w = |id: i32, team: i32, elo: f64, shift: f64| WhatIf {
            player_id: PlayerId::new(id),
            name: format!("Player {id}"),
            team_id: TeamId::new(team),
            team_code: format!("T{team}"),
            elo,
            last_games: None,
            basis: None,
            current_elo: Some(elo - shift),
            current_source: Some(WorthSource::Box),
            shift_elo: shift,
        };
        let what_ifs = vec![
            w(1, 1, 60.0, 53.0),
            w(2, 1, 30.0, -10.0),
            w(3, 2, 400.0, 400.0),
            w(4, 2, 50.0, 50.0),
        ];
        let shifts = what_if_shifts(&what_ifs);
        assert_eq!(shifts.get(&TeamId::new(1)), Some(&43.0));
        // Two players on one team cannot push it past the cap.
        assert_eq!(shifts.get(&TeamId::new(2)), Some(&400.0));
        let resolved = resolved_worths(&what_ifs);
        assert_eq!(resolved.len(), 4);
        assert_eq!(resolved[0], (PlayerId::new(1), 60.0));
        assert!(what_if_shifts(&[]).is_empty());
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
    fn the_box_estimate_pools_the_seasons_and_their_stints_by_minutes() {
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
        // Each season over its stints weighted by minutes (300 at 15 and 900 at 45 give
        // 37.5 in 2026), then the seasons by minutes: 1,200 at 37.5 and 2,000 at 80.
        let e = pick_box_estimate(&tables, PlayerId::new(1)).unwrap();
        assert_eq!(
            (e.elo, e.se_elo, e.season, e.first_season),
            (
                (1200.0 * 37.5 + 2000.0 * 80.0) / 3200.0,
                0.0,
                SeasonYear::of(2026),
                SeasonYear::of(2025)
            )
        );
        assert_eq!(e.source, WorthSource::Box);
        assert_eq!(pick_box_estimate(&tables, PlayerId::new(9)), None);
    }

    #[test]
    fn the_estimates_follow_the_old_rule_without_a_calibration_and_combine_with_one() {
        let est = |elo: f64, se: f64, source| Estimate {
            elo,
            se_elo: se,
            season: SeasonYear::of(2025),
            first_season: SeasonYear::of(2025),
            source,
        };
        let noisy = est(-16.0, 70.0, WorthSource::Worth);
        let clear = est(60.0, 20.0, WorthSource::Worth);
        let boxed = est(40.0, 0.0, WorthSource::Box);
        // Without a calibration, the older rule.
        assert_eq!(choose_estimate(Some(noisy), Some(boxed), None), Some(boxed));
        assert_eq!(choose_estimate(Some(clear), Some(boxed), None), Some(clear));
        assert_eq!(choose_estimate(Some(noisy), None, None), Some(noisy));
        assert_eq!(choose_estimate(None, Some(boxed), None), Some(boxed));
        assert_eq!(choose_estimate(None, None, None), None);
        // With one, the box worth is scaled and carries the error, and the two combine.
        let c = BoxCalibration {
            model: "test",
            pairs: 40,
            slope: 0.5,
            slope_se: 0.0,
            error_elo: 15.0,
        };
        let alone = choose_estimate(None, Some(boxed), Some(&c)).unwrap();
        assert_eq!(
            (alone.elo, alone.se_elo, alone.source),
            (20.0, 15.0, WorthSource::Box)
        );
        let both = choose_estimate(Some(noisy), Some(boxed), Some(&c)).unwrap();
        assert_eq!(both.source, WorthSource::Blend);
        assert!(
            both.elo > 15.0 && both.elo < 20.0,
            "pulled a little toward the noisy -16"
        );
        assert!(both.se_elo < 15.0);
        // A clear worth (60 ± 20) against a more precise scaled box worth (20 ± 15): the
        // box side weighs more, and the result sits between the two.
        let clear_both = choose_estimate(Some(clear), Some(boxed), Some(&c)).unwrap();
        assert!(
            clear_both.elo > 30.0 && clear_both.elo < 40.0,
            "{}",
            clear_both.elo
        );
        assert_eq!(choose_estimate(Some(noisy), None, Some(&c)), Some(noisy));
    }
}
