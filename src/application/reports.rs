//! Game reports: a link to the wire-service report of a played game, shown beside its score.
//! The pages ask for the dates they show; this use case answers from the store and asks the
//! report source for the dates it has not looked up yet, a few per request, then matches the
//! source's games to the stored games of that date by team name or code and keeps the links.
//! A date with no report is kept as looked up, and asked again only while the games are
//! recent enough for a report to appear later.

use std::collections::{BTreeMap, HashMap};

use chrono::{Duration, NaiveDate, Utc};
use serde::Serialize;
use tracing::{info, warn};

use crate::domain::models::Team;

use super::ports::{
    GameReport, ReportCandidate, ReportLookup, ReportSource, ReportStore, SourceError, StoreError,
};

/// Dates one request may ask for.
pub const MAX_DATES: usize = 40;
/// Dates one request fetches from the source; the rest wait for the next request.
pub const MAX_FETCHES: usize = 6;
/// Time one request spends fetching; the rest of its dates wait for the next request.
pub const FETCH_BUDGET: std::time::Duration = std::time::Duration::from_secs(15);
/// A date without reports is asked again after this long, while its games are recent.
const RETRY_AFTER: Duration = Duration::hours(6);
/// How long after a game a report may still appear.
const RECENT: Duration = Duration::days(4);

/// The source's abbreviations that differ from the dataset's codes.
const ABBREVIATIONS: &[(&str, &str)] = &[
    ("GS", "GSW"),
    ("SA", "SAS"),
    ("NY", "NYK"),
    ("NO", "NOP"),
    ("UTAH", "UTA"),
    ("WSH", "WAS"),
    ("BKN", "BRK"),
    ("CHA", "CHO"),
    ("PHX", "PHO"),
    ("NJ", "NJN"),
];

#[derive(Debug, thiserror::Error)]
pub enum ReportError {
    #[error("at most {MAX_DATES} dates per request")]
    TooManyDates,
    #[error(transparent)]
    Store(#[from] StoreError),
}

/// What a page gets: the reports per date, and the dates still to be fetched.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Reports {
    pub reports: BTreeMap<NaiveDate, Vec<GameReport>>,
    pub pending: Vec<NaiveDate>,
    /// No source is configured: the page shows no links and asks no more.
    pub disabled: bool,
}

/// A played game of a date, by the dataset's codes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlayedGame {
    pub home_code: String,
    pub away_code: String,
}

/// The dataset codes a candidate side may refer to: the team whose name matches, the
/// abbreviation mapped where the source differs, and the abbreviation itself. The store
/// may hold a game under the franchise's current code while the source names the team of
/// the day (the Charlotte Bobcats under CHO), so every candidate counts.
fn codes_of(abbr: &str, name: &str, by_name: &HashMap<String, String>) -> Vec<String> {
    let mut codes = Vec::new();
    if let Some(code) = by_name.get(&name.trim().to_ascii_lowercase()) {
        codes.push(code.clone());
    }
    let abbr = abbr.trim().to_ascii_uppercase();
    if let Some((_, to)) = ABBREVIATIONS.iter().find(|(from, _)| *from == abbr) {
        codes.push(to.to_string());
    }
    if !abbr.is_empty() && !codes.contains(&abbr) {
        codes.push(abbr);
    }
    codes
}

/// Match the source's games to the stored games of the date. A candidate matches a game
/// when both sides can refer to the game's teams, in either order (sources disagree on
/// the home team at a neutral site).
pub fn match_candidates(
    date: NaiveDate,
    source: &str,
    candidates: &[ReportCandidate],
    games: &[PlayedGame],
    teams: &[Team],
) -> Vec<GameReport> {
    let by_name: HashMap<String, String> = teams
        .iter()
        .map(|t| (t.name.trim().to_ascii_lowercase(), t.code.clone()))
        .collect();
    let mut out: Vec<GameReport> = Vec::new();
    for c in candidates {
        let h = codes_of(&c.home_abbr, &c.home_name, &by_name);
        let a = codes_of(&c.away_abbr, &c.away_name, &by_name);
        let Some(g) = games.iter().find(|g| {
            (h.contains(&g.home_code) && a.contains(&g.away_code))
                || (a.contains(&g.home_code) && h.contains(&g.away_code))
        }) else {
            continue;
        };
        if out
            .iter()
            .any(|r| r.home_code == g.home_code && r.away_code == g.away_code)
        {
            continue;
        }
        out.push(GameReport {
            game_date: date,
            home_code: g.home_code.clone(),
            away_code: g.away_code.clone(),
            url: c.url.clone(),
            title: c.title.clone(),
            source: source.to_string(),
        });
    }
    out
}

/// Whether a date needs (another) lookup.
fn needs_lookup(
    date: NaiveDate,
    lookup: Option<&ReportLookup>,
    now: chrono::DateTime<Utc>,
) -> bool {
    match lookup {
        None => true,
        Some(l) if l.found > 0 => false,
        Some(l) => {
            let recent = now.date_naive() - date <= RECENT;
            recent && now - l.fetched_at >= RETRY_AFTER
        }
    }
}

/// Where the lookups of one request may go.
#[derive(Debug, Clone, Copy)]
pub enum Fetching<'a, R> {
    /// No source is configured: answer from the store, and tell the page so.
    Off,
    /// Another request holds the lookup slot: answer from the store, the rest is pending.
    Busy,
    /// Look up the dates the store does not have, within the request's budget.
    With(&'a R),
}

/// The reports for `dates`. `games` gives the played games of each date.
pub async fn for_dates<S, R>(
    store: &S,
    fetching: Fetching<'_, R>,
    teams: &[Team],
    games: &BTreeMap<NaiveDate, Vec<PlayedGame>>,
    dates: &[NaiveDate],
) -> Result<Reports, ReportError>
where
    S: ReportStore,
    R: ReportSource,
{
    if dates.len() > MAX_DATES {
        return Err(ReportError::TooManyDates);
    }
    let mut out = Reports {
        disabled: matches!(fetching, Fetching::Off),
        ..Default::default()
    };
    for r in store.reports_on(dates).await? {
        out.reports.entry(r.game_date).or_default().push(r);
    }
    if matches!(fetching, Fetching::Off) {
        return Ok(out);
    }
    let lookups: HashMap<NaiveDate, ReportLookup> = store
        .lookups_on(dates)
        .await?
        .into_iter()
        .map(|l| (l.game_date, l))
        .collect();
    let now = Utc::now();
    let started = std::time::Instant::now();
    let mut fetched = 0usize;
    let mut stopped = false;
    for &date in dates {
        let played = games.get(&date).map(Vec::as_slice).unwrap_or(&[]);
        if played.is_empty() || !needs_lookup(date, lookups.get(&date), now) {
            continue;
        }
        let Fetching::With(source) = fetching else {
            out.pending.push(date);
            continue;
        };
        if stopped || fetched >= MAX_FETCHES || started.elapsed() >= FETCH_BUDGET {
            out.pending.push(date);
            continue;
        }
        fetched += 1;
        match source.reports_on(date).await {
            Ok(listing) => {
                let reports =
                    match_candidates(date, source.name(), &listing.reports, played, teams);
                info!(%date, listed = listing.games_listed, reports = listing.reports.len(), matched = reports.len(), "game reports looked up");
                // A feed that lists no game for a date with played games may be a glitch:
                // nothing is recorded, so the date is asked again.
                if listing.games_listed > 0 {
                    store.save_lookup(date, source.name(), &reports).await?;
                }
                if !reports.is_empty() {
                    out.reports.insert(date, reports);
                }
            }
            Err(e) => {
                // A failed lookup is not recorded: a later request tries again. After a
                // transport failure or a rejection the request stops asking, and the dates
                // it did not reach wait for a later request.
                warn!(%date, error = %e, "game report lookup failed");
                if matches!(e, SourceError::Rejected { .. } | SourceError::Transport(_)) {
                    stopped = true;
                }
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ids::{FranchiseId, SeasonYear, TeamId};

    fn team(code: &str, name: &str) -> Team {
        Team {
            id: TeamId::new(1),
            code: code.to_string(),
            franchise_id: FranchiseId::new(1),
            name: name.to_string(),
            first_season: Some(SeasonYear::of(2000)),
            last_season: None,
        }
    }

    fn candidate(home: (&str, &str), away: (&str, &str), url: &str) -> ReportCandidate {
        ReportCandidate {
            home_abbr: home.0.into(),
            away_abbr: away.0.into(),
            home_name: home.1.into(),
            away_name: away.1.into(),
            url: url.into(),
            title: Some("t".into()),
        }
    }

    fn game(home: &str, away: &str) -> PlayedGame {
        PlayedGame {
            home_code: home.into(),
            away_code: away.into(),
        }
    }

    #[test]
    fn candidates_match_by_name_then_code_in_either_order() {
        let teams = vec![
            team("GSW", "Golden State Warriors"),
            team("DAL", "Dallas Mavericks"),
            team("NOP", "New Orleans Pelicans"),
            team("UTA", "Utah Jazz"),
            team("CHO", "Charlotte Hornets"),
            team("CHA", "Charlotte Bobcats"),
            team("NOH", "New Orleans Hornets"),
        ];
        let date = NaiveDate::from_ymd_opt(2019, 3, 23).unwrap();
        let candidates = vec![
            candidate(
                ("GS", "Golden State Warriors"),
                ("DAL", "Dallas Mavericks"),
                "https://www.espn.com/r/1",
            ),
            // Unknown names: the abbreviations decide, mapped to the dataset's codes.
            candidate(("UTAH", "?"), ("NO", "?"), "https://www.espn.com/r/2"),
            // The source names the team of the day; the store holds the game under the
            // franchise's current code: the mapped abbreviation still matches.
            candidate(
                ("CHA", "Charlotte Bobcats"),
                ("NO", "New Orleans Hornets"),
                "https://www.espn.com/r/5",
            ),
            // Home and away swapped against the store (a neutral site): still matched.
            candidate(
                ("DAL", "Dallas Mavericks"),
                ("GS", "Golden State Warriors"),
                "https://www.espn.com/r/3",
            ),
            // No such game that day.
            candidate(
                ("BOS", "Boston Celtics"),
                ("MIA", "Miami Heat"),
                "https://www.espn.com/r/4",
            ),
        ];
        let games = vec![game("GSW", "DAL"), game("NOP", "UTA"), game("CHO", "NOP")];
        let r = match_candidates(date, "espn", &candidates, &games, &teams);
        assert_eq!(r.len(), 3);
        assert_eq!(
            (r[2].home_code.as_str(), r[2].away_code.as_str()),
            ("CHO", "NOP")
        );
        assert_eq!(
            (
                r[0].home_code.as_str(),
                r[0].away_code.as_str(),
                r[0].url.as_str()
            ),
            ("GSW", "DAL", "https://www.espn.com/r/1")
        );
        assert_eq!(
            (r[1].home_code.as_str(), r[1].away_code.as_str()),
            ("NOP", "UTA")
        );
        assert_eq!(r[1].source, "espn");
    }

    #[test]
    fn lookups_repeat_only_while_the_games_are_recent() {
        let now = Utc::now();
        let today = now.date_naive();
        let lookup = |date: NaiveDate, found: i32, hours_ago: i64| ReportLookup {
            game_date: date,
            source: "espn".into(),
            fetched_at: now - Duration::hours(hours_ago),
            found,
        };
        let old = NaiveDate::from_ymd_opt(2010, 3, 13).unwrap();
        assert!(needs_lookup(old, None, now));
        assert!(
            !needs_lookup(old, Some(&lookup(old, 0, 100)), now),
            "an old date without reports is final"
        );
        assert!(!needs_lookup(old, Some(&lookup(old, 3, 1)), now));
        let yesterday = today - Duration::days(1);
        assert!(
            !needs_lookup(yesterday, Some(&lookup(yesterday, 0, 1)), now),
            "asked an hour ago"
        );
        assert!(
            needs_lookup(yesterday, Some(&lookup(yesterday, 0, 7)), now),
            "asked again after six hours"
        );
    }
}
