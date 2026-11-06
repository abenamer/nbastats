//! Use case: backfill the market record from the sportsbook's history. For each night in a
//! range with stored regular-season games that have no snapshot yet, the night's tip-off
//! times are read from the history source, then its lines as they stood a quarter of an
//! hour before the first tip-off (or before each tip-off), and each priced game is stored
//! as the live capture stores a night. The model's side is the stored pre-game Elo alone,
//! with the season's fitted home bonus, under its own model version
//! (`domain::tracking::ELO_ONLY_MODEL`): a past night has no injury report to price. The
//! store's rule keeps a later capture over an earlier one, so a night the live task
//! captured is left as it is, and a run can be repeated: a night whose games all have a
//! snapshot costs nothing. The history costs the service's credits by the market, so a
//! dry run says what the nights would cost, and a run stops when the credits left do not
//! cover the next read.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::time::Duration;

use chrono::{DateTime, NaiveDate, Utc};
use serde::Serialize;

use crate::domain::calendar;
use crate::domain::ids::{SeasonYear, TeamId};
use crate::domain::market::DEFAULT_MIN_RETURN;
use crate::domain::models::{Game, GameType};
use crate::domain::tracking;
use crate::domain::worth as worth_domain;

use super::market::{self, Market};
use super::ports::{GameLog, MarketHistory, MarketStore, OddsMarket, SourceError, StoreError};
use super::scheduler::current_season;
use super::state::{App, SeasonLoadError};
use super::tracking::snapshots_of;
use super::worth::GameTonight;

/// How long before a tip-off the lines are read, in minutes: about where the live capture
/// reads them, within eleven minutes of the tip-off plus the lines' five in the cache.
pub const LEAD_MINUTES: i64 = 15;
/// The hour of the league's day, Eastern, the tip-off times are read at: the day's games
/// are listed by then and none has tipped off.
pub const LISTING_HOUR: i64 = 10;
/// What the service bills: ten credits per market in a history read, one for a listing.
pub const CREDITS_PER_MARKET: u64 = 10;
pub const CREDITS_PER_LISTING: u64 = 1;
/// The tip-off times a night has, as the estimate counts them before they are read.
pub const TYPICAL_TIP_OFFS: u64 = 5;
/// The most reads a night gets when read per tip-off, whatever the service lists: a night
/// has at most about eight distinct tip-off times.
pub const MAX_READS_PER_NIGHT: usize = 16;
/// Nights failing one after another before the run stops: the service, or the way to it,
/// is not answering, and every try may cost a credit.
pub const MAX_FAILED_NIGHTS_IN_A_ROW: u32 = 5;
/// The pause between two reads of the service.
const PAUSE: Duration = Duration::from_millis(250);

/// How a run reads the history: the lines before the night's first tip-off, or before each
/// of its tip-off times, in the markets named.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Plan {
    pub per_tip_off: bool,
    pub markets: Vec<OddsMarket>,
}

impl Plan {
    /// The credits one read of the lines costs.
    pub fn read_cost(&self) -> u64 {
        CREDITS_PER_MARKET * self.markets.len() as u64
    }

    /// When the lines are read for a night with these tip-off times: [`LEAD_MINUTES`]
    /// before the first, or before each distinct one, in order.
    pub fn read_times(&self, tip_offs: &[DateTime<Utc>]) -> Vec<DateTime<Utc>> {
        let mut times: Vec<DateTime<Utc>> = tip_offs
            .iter()
            .map(|t| *t - chrono::Duration::minutes(LEAD_MINUTES))
            .collect();
        times.sort();
        times.dedup();
        times.truncate(if self.per_tip_off {
            MAX_READS_PER_NIGHT
        } else {
            1
        });
        times
    }

    /// The credits `nights` nights cost before their tip-offs are read: a listing and a
    /// read each, or [`TYPICAL_TIP_OFFS`] reads when read per tip-off.
    pub fn estimate(&self, nights: u64) -> u64 {
        let reads = if self.per_tip_off {
            TYPICAL_TIP_OFFS
        } else {
            1
        };
        nights * (CREDITS_PER_LISTING + reads * self.read_cost())
    }
}

/// The time of day the tip-off times of `date` are read at.
pub fn listing_time(date: NaiveDate) -> DateTime<Utc> {
    calendar::day_span(date).0 + chrono::Duration::hours(LISTING_HOUR)
}

/// One night as the run saw it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NightOutcome {
    pub date: NaiveDate,
    pub season: SeasonYear,
    /// The night's stored regular-season games, and those that had a snapshot already.
    pub games: u32,
    pub stored_before: u32,
    /// The tip-off times the service listed, the reads made, the rows written and the
    /// games left without a snapshot.
    pub tip_offs: u32,
    pub reads: u32,
    pub written: u64,
    pub unpriced: u32,
    pub note: Option<String>,
}

/// What a run did.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Outcome {
    pub from: NaiveDate,
    pub to: NaiveDate,
    pub plan: Plan,
    pub dry_run: bool,
    pub model_version: String,
    pub nights: Vec<NightOutcome>,
    /// Nights read (or, in a dry run, to read) and nights skipped for having every snapshot.
    pub nights_to_read: u32,
    pub nights_skipped: u32,
    pub written: u64,
    /// The credits the nights to read cost by the estimate, before any was read.
    pub estimated_credits: u64,
    /// The run's own budget, when one was set, and the credits its requests cost by the
    /// service's rates, counted whether or not the service answered.
    pub max_credits: Option<u64>,
    pub credits_spent: u64,
    /// The service's credits left after the last read, when it said.
    pub credits_remaining: Option<u64>,
    /// Why the run stopped before the range's end, when it did.
    pub stopped: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum BackfillError {
    #[error("--from must not be after --to")]
    Range,
    #[error("the plan names no market")]
    NoMarkets,
    #[error(transparent)]
    Season(#[from] SeasonLoadError),
}

impl From<StoreError> for BackfillError {
    fn from(e: StoreError) -> Self {
        BackfillError::Season(e.into())
    }
}

/// A stored game by date and team codes, as the snapshots are keyed.
type Key = (NaiveDate, String, String);

/// The credits left as The Odds API last reported them.
fn credits_left(app: &App) -> Option<u64> {
    app.odds
        .as_ref()
        .and_then(|o| o.quota())
        .and_then(|q| q.remaining)
}

/// Why the next request, of `cost` credits, cannot be made: the run's `budget` would be
/// passed, counting the `spent` so far, or the credits `left` at the service do not cover
/// it. None when it can.
pub fn short(spent: u64, cost: u64, budget: Option<u64>, left: Option<u64>) -> Option<String> {
    if let Some(budget) = budget {
        if spent.saturating_add(cost) > budget {
            return Some(format!(
                "{spent} credits spent of the {budget} allowed; the next request costs {cost}"
            ));
        }
    }
    if let Some(left) = left {
        if left < cost {
            return Some(format!(
                "{left} credits left at the service do not cover the next request of {cost}"
            ));
        }
    }
    None
}

/// Whether an error from the service says the run cannot go on: the key or the plan
/// refused (401, 402, 403), the request itself refused (422, as for a date the history
/// does not hold), the rate limit (429), or no source configured. A transport or payload
/// error is a night's failure, not the run's.
pub fn fatal(e: &SourceError) -> bool {
    match e {
        SourceError::Rejected { status, .. } => matches!(status, 401 | 402 | 403 | 422 | 429),
        SourceError::NotConfigured(_) => true,
        SourceError::RateLimited(_) => true,
        SourceError::Transport(_) | SourceError::InvalidPayload(_) => false,
    }
}

/// The nights of `games` from `from` to `to`, each with its played regular-season games.
pub fn nights_of(
    games: &[Game],
    from: NaiveDate,
    to: NaiveDate,
) -> BTreeMap<NaiveDate, Vec<&Game>> {
    let mut by_date: BTreeMap<NaiveDate, Vec<&Game>> = BTreeMap::new();
    for g in games.iter().filter(|g| {
        g.game_type == GameType::Regular
            && g.is_played()
            && g.game_date >= from
            && g.game_date <= to
    }) {
        by_date.entry(g.game_date).or_default().push(g);
    }
    by_date
}

/// Backfill the record from `from` to `to`, both inclusive, reading `history` as `plan`
/// says and spending at most `max_credits` by the service's rates when one is given.
/// `progress` hears each night.
#[allow(clippy::too_many_arguments)]
pub async fn backfill<S: MarketHistory>(
    app: &App,
    history: &S,
    from: NaiveDate,
    to: NaiveDate,
    plan: &Plan,
    dry_run: bool,
    max_credits: Option<u64>,
    progress: &(dyn Fn(&str) + Sync),
) -> Result<Outcome, BackfillError> {
    if from > to {
        return Err(BackfillError::Range);
    }
    if plan.markets.is_empty() {
        return Err(BackfillError::NoMarkets);
    }
    let mut spent: u64 = 0;
    let mut failed_nights: u32 = 0;
    let teams = app.store.team_code_map().await?;
    let codes: HashMap<TeamId, String> =
        teams.iter().map(|(code, t)| (t.id, code.clone())).collect();
    let by_name: HashMap<String, String> = teams
        .iter()
        .map(|(code, t)| (t.name.trim().to_ascii_lowercase(), code.clone()))
        .collect();
    let model_version = tracking::elo_only_model_version();
    let mut out = Outcome {
        from,
        to,
        plan: plan.clone(),
        dry_run,
        model_version: model_version.clone(),
        nights: Vec::new(),
        nights_to_read: 0,
        nights_skipped: 0,
        written: 0,
        estimated_credits: 0,
        max_credits,
        credits_spent: 0,
        credits_remaining: credits_left(app),
        stopped: None,
    };
    'seasons: for season in SeasonYear::range(current_season(from), current_season(to)) {
        let data = match app.season_data(season).await {
            Ok(d) => d,
            Err(SeasonLoadError::NoGames(_)) => continue,
            Err(e) => return Err(e.into()),
        };
        let nights = nights_of(&data.games, from, to);
        if nights.is_empty() {
            continue;
        }
        let existing: HashSet<Key> = app
            .store
            .snapshots(season)
            .await?
            .into_iter()
            .map(|s| (s.game_date, s.home_code, s.away_code))
            .collect();
        let home_advantage = app.home_advantage(season).await?.regular;
        let scatter = market::margin_scatter(app, season).await?;
        let key = |g: &Game| -> Option<Key> {
            Some((
                g.game_date,
                codes.get(&g.home_team_id)?.clone(),
                codes.get(&g.away_team_id)?.clone(),
            ))
        };
        for (date, games) in nights {
            let stored_before = games
                .iter()
                .filter(|g| key(g).is_some_and(|k| existing.contains(&k)))
                .count() as u32;
            let mut night = NightOutcome {
                date,
                season,
                games: games.len() as u32,
                stored_before,
                tip_offs: 0,
                reads: 0,
                written: 0,
                unpriced: games.len() as u32 - stored_before,
                note: None,
            };
            if night.unpriced == 0 {
                night.note = Some("every game has a snapshot".to_string());
                progress(&format!(
                    "{date}: {} games, all stored; skipped",
                    night.games
                ));
                out.nights_skipped += 1;
                out.nights.push(night);
                continue;
            }
            out.nights_to_read += 1;
            out.estimated_credits += plan.estimate(1);
            if dry_run {
                progress(&format!(
                    "{date}: {} games, {} without a snapshot; would read",
                    night.games, night.unpriced
                ));
                out.nights.push(night);
                continue;
            }
            // The credits must cover the listing and one read before the night is begun: the
            // run's own budget first, then what the service says is left.
            let night_cost = CREDITS_PER_LISTING + plan.read_cost();
            if let Some(why) = short(spent, night_cost, max_credits, credits_left(app)) {
                out.stopped = Some(format!("stopped before {date}: {why}"));
                out.nights.push(night);
                break 'seasons;
            }
            spent += CREDITS_PER_LISTING;
            let tip_offs = match history.tip_offs_at(date, listing_time(date)).await {
                Ok(t) => t,
                Err(e) => {
                    night.note = Some(format!("the tip-off times could not be read: {e}"));
                    progress(&format!("{date}: {}", night.note.as_deref().unwrap_or("")));
                    failed_nights += 1;
                    if fatal(&e) {
                        out.stopped = Some(format!("stopped at {date}: the service refused: {e}"));
                    } else if failed_nights >= MAX_FAILED_NIGHTS_IN_A_ROW {
                        out.stopped = Some(format!(
                            "stopped at {date}: {failed_nights} nights failed in a row"
                        ));
                    }
                    out.credits_spent = spent;
                    out.nights.push(night);
                    if out.stopped.is_some() {
                        break 'seasons;
                    }
                    tokio::time::sleep(PAUSE).await;
                    continue;
                }
            };
            tokio::time::sleep(PAUSE).await;
            night.tip_offs = tip_offs.len() as u32;
            if tip_offs.is_empty() {
                // An answered listing, so the failure streak ends; it was billed all the same.
                failed_nights = 0;
                out.credits_spent = spent;
                night.note = Some("the service lists no game for the date".to_string());
                progress(&format!("{date}: {}", night.note.as_deref().unwrap_or("")));
                out.nights.push(night);
                continue;
            }
            // The night's played games priced at their stored pre-game ratings: the Elo
            // alone. A game of the date not yet played has no result to score and no
            // rating here, so it is left out of the slate.
            let ratings: HashMap<TeamId, f64> = games
                .iter()
                .flat_map(|g| {
                    [
                        (g.home_team_id, g.home_elo_pre),
                        (g.away_team_id, g.away_elo_pre),
                    ]
                })
                .collect();
            let played_ids: HashSet<i64> = games.iter().map(|g| g.id).collect();
            let slate: Vec<GameTonight> =
                worth_domain::night_of(&data.games, date, &ratings, &[], home_advantage)
                    .into_iter()
                    .filter(|game| played_ids.contains(&game.game_id))
                    .map(|game| GameTonight {
                        home_code: codes
                            .get(&game.home_team_id)
                            .cloned()
                            .unwrap_or_else(|| game.home_team_id.to_string()),
                        away_code: codes
                            .get(&game.away_team_id)
                            .cloned()
                            .unwrap_or_else(|| game.away_team_id.to_string()),
                        game,
                    })
                    .collect();
            let mut priced: HashSet<i64> = HashSet::new();
            let mut notes = Vec::new();
            for at in plan.read_times(&tip_offs) {
                if let Some(why) = short(spent, plan.read_cost(), max_credits, credits_left(app)) {
                    out.stopped = Some(format!("stopped during {date}: {why}"));
                    notes.push(format!(
                        "the night's later reads were not made ({why}); a later run reads the night as whole when every game has a row"
                    ));
                    break;
                }
                spent += plan.read_cost();
                match history.lines_at(date, at, &plan.markets).await {
                    Ok((read_at, lines)) => {
                        night.reads += 1;
                        let matched = market::match_lines(
                            &lines,
                            &slate,
                            &by_name,
                            scatter.sigma,
                            DEFAULT_MIN_RETURN,
                        );
                        let m = Market {
                            season,
                            night: Some(date),
                            read_at: Some(read_at),
                            source: Some(history.name()),
                            source_note: None,
                            quota: None,
                            min_return: DEFAULT_MIN_RETURN,
                            scatter,
                            games: matched,
                            note: None,
                        };
                        // A game that has a row already, the live capture's or an earlier
                        // run's, keeps it: the backfill fills gaps and replaces nothing.
                        let snapshots: Vec<_> = snapshots_of(
                            season,
                            date,
                            home_advantage,
                            &slate,
                            &ratings,
                            &m,
                            &model_version,
                        )
                        .into_iter()
                        .filter(|s| {
                            !existing.contains(&(
                                s.game_date,
                                s.home_code.clone(),
                                s.away_code.clone(),
                            ))
                        })
                        .collect();
                        for s in &snapshots {
                            if let Some(g) = slate
                                .iter()
                                .find(|g| g.home_code == s.home_code && g.away_code == s.away_code)
                            {
                                priced.insert(g.game.game_id);
                            }
                        }
                        night.written += app.store.save_snapshots(&snapshots).await?;
                    }
                    Err(e) => {
                        notes.push(format!("a read at {at} failed: {e}"));
                        if fatal(&e) {
                            out.stopped =
                                Some(format!("stopped during {date}: the service refused: {e}"));
                            notes.push("the night's later reads were not made".to_string());
                            break;
                        }
                    }
                }
                tokio::time::sleep(PAUSE).await;
            }
            if night.reads > 0 {
                failed_nights = 0;
            } else {
                failed_nights += 1;
                if out.stopped.is_none() && failed_nights >= MAX_FAILED_NIGHTS_IN_A_ROW {
                    out.stopped = Some(format!(
                        "stopped at {date}: {failed_nights} nights failed in a row"
                    ));
                }
            }
            out.credits_spent = spent;
            night.unpriced = games
                .iter()
                .filter(|g| {
                    !priced.contains(&g.id) && !key(g).is_some_and(|k| existing.contains(&k))
                })
                .count() as u32;
            if !notes.is_empty() {
                night.note = Some(notes.join("; "));
            }
            out.written += night.written;
            out.credits_remaining = credits_left(app);
            progress(&format!(
                "{date}: {} games, {} tip-off times, {} reads, {} rows written, {} unpriced{}{}",
                night.games,
                night.tip_offs,
                night.reads,
                night.written,
                night.unpriced,
                out.credits_remaining
                    .map(|c| format!(", {c} credits left"))
                    .unwrap_or_default(),
                night
                    .note
                    .as_deref()
                    .map(|n| format!("; {n}"))
                    .unwrap_or_default()
            ));
            let stopped = out.stopped.is_some();
            out.nights.push(night);
            if stopped {
                break 'seasons;
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::models::{EloSource, ResultSource};

    fn at(s: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
    }

    #[test]
    fn a_plan_reads_before_the_first_tip_off_or_before_each_and_costs_by_the_market() {
        let tips = vec![
            at("2021-01-16T02:10:00Z"),
            at("2021-01-16T00:30:00Z"),
            at("2021-01-16T00:30:00Z"),
            at("2021-01-16T03:10:00Z"),
        ];
        let night = Plan {
            per_tip_off: false,
            markets: vec![OddsMarket::Moneyline],
        };
        assert_eq!(night.read_times(&tips), vec![at("2021-01-16T00:15:00Z")]);
        assert_eq!(night.read_cost(), 10);
        // A season of 165 nights at one moneyline read a night: 165 × 11.
        assert_eq!(night.estimate(165), 1815);
        let each = Plan {
            per_tip_off: true,
            markets: vec![OddsMarket::Moneyline, OddsMarket::Spread, OddsMarket::Total],
        };
        assert_eq!(
            each.read_times(&tips),
            vec![
                at("2021-01-16T00:15:00Z"),
                at("2021-01-16T01:55:00Z"),
                at("2021-01-16T02:55:00Z")
            ]
        );
        assert_eq!((each.read_cost(), each.estimate(13)), (30, 13 * 151));
        assert!(night.read_times(&[]).is_empty());
        // A night with more distinct tip-off times than the cap is read the cap's times.
        let many: Vec<DateTime<Utc>> = (0..40)
            .map(|i| at("2021-01-16T00:00:00Z") + chrono::Duration::minutes(10 * i))
            .collect();
        assert_eq!(each.read_times(&many).len(), MAX_READS_PER_NIGHT);
        // The budget and the service's count both stop a request; neither alone passes it.
        assert_eq!(short(0, 11, None, None), None);
        assert_eq!(short(1980, 11, Some(2000), None), None);
        assert!(
            short(1980, 11, Some(2000), None).is_none()
                && short(1980, 11, Some(2000), Some(10)).is_some()
        );
        assert!(short(1995, 11, Some(2000), Some(5000)).is_some());
        assert!(short(0, 11, None, Some(10)).is_some());
        assert!(short(u64::MAX, 1, Some(u64::MAX - 1), None).is_some());
        assert_eq!(
            short(u64::MAX, 1, Some(u64::MAX), None),
            None,
            "no overflow"
        );
        // Refusals and the rate limit stop the run; a transport or payload error does not.
        for status in [401, 402, 403, 422, 429] {
            assert!(fatal(&SourceError::Rejected {
                status,
                message: String::new()
            }));
        }
        assert!(!fatal(&SourceError::Rejected {
            status: 500,
            message: String::new()
        }));
        assert!(!fatal(&SourceError::Transport("down".into())));
        assert!(!fatal(&SourceError::InvalidPayload("odd".into())));
        assert!(fatal(&SourceError::NotConfigured("no key".into())));
        // The listing is read at ten in the morning, Eastern: 15:00 UTC in November.
        assert_eq!(
            listing_time(NaiveDate::from_ymd_opt(2026, 11, 2).unwrap()),
            at("2026-11-02T15:00:00Z")
        );
    }

    fn game(id: i64, date: &str, home: i32, away: i32, played: bool, kind: GameType) -> Game {
        Game {
            id,
            season: SeasonYear::of(2027),
            game_date: NaiveDate::parse_from_str(date, "%Y-%m-%d").unwrap(),
            seq: 0,
            home_team_id: TeamId::new(home),
            away_team_id: TeamId::new(away),
            neutral: false,
            game_type: kind,
            home_score: played.then_some(100),
            away_score: played.then_some(90),
            home_elo_pre: 1500.0,
            away_elo_pre: 1500.0,
            home_elo_post: 1500.0,
            away_elo_post: 1500.0,
            home_win_prob: None,
            elo_source: EloSource::Computed,
            result_source: ResultSource::Results,
            ref_home_elo_pre: None,
            ref_away_elo_pre: None,
            ref_home_elo_post: None,
            ref_away_elo_post: None,
            cup_stage: None,
        }
    }

    #[test]
    fn the_nights_are_the_dates_in_range_with_played_regular_season_games() {
        let games = vec![
            game(1, "2026-10-20", 1, 2, true, GameType::Regular),
            game(2, "2026-10-20", 3, 4, true, GameType::Regular),
            game(3, "2026-10-21", 1, 3, false, GameType::Regular),
            game(4, "2026-10-22", 2, 4, true, GameType::Playoff),
            game(5, "2026-10-23", 1, 4, true, GameType::Regular),
            game(6, "2026-10-24", 2, 3, true, GameType::Regular),
        ];
        let d = |s: &str| NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap();
        let nights = nights_of(&games, d("2026-10-20"), d("2026-10-23"));
        let dates: Vec<(NaiveDate, usize)> = nights.iter().map(|(k, v)| (*k, v.len())).collect();
        assert_eq!(dates, vec![(d("2026-10-20"), 2), (d("2026-10-23"), 1)]);
    }
}
