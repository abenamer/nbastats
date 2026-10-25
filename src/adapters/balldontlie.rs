//! Outbound adapter: game results and schedule from the BALLDONTLIE NBA API (v1).
//!
//! `GET /v1/games?seasons[]=<start year>&per_page=100&cursor=<n>` pages through a season;
//! `start_date` and `end_date` bound a daily refresh to the days since the last result.
//! The same adapter is the player source: `GET /v1/stats?dates[]=<date>` gives the box
//! scores of a date, `GET /v1/player_injuries` the injury report and `GET /v1/teams` the
//! source's team identifiers (these endpoints are on the paid plans).
//! The free tier allows 5 requests per minute, so requests are paced and a 429 is retried
//! after the `Retry-After` delay. Team abbreviations differ from the Elo dataset for three
//! teams (BKN, CHA, PHX); `dataset_code` maps them.

use std::collections::HashMap;
use std::time::Duration;

use chrono::NaiveDate;
use serde::Deserialize;
use tokio::sync::Mutex;
use tokio::time::Instant;

use crate::application::ports::{
    BoxScoreLine, FetchProgress, FetchProgressSink, FetchWindow, InjuryRecord, PlayerRecord,
    PlayerSource, ResultRecord, ResultsFetch, ResultsSource, SourceError,
};
use crate::domain::ids::SeasonYear;

const PER_PAGE: u32 = 100;
const MAX_PAGES: u32 = 200;
const MAX_RATE_LIMIT_RETRIES: u32 = 6;
/// Upper bound on one page's body (a 100-game page is about 60 KB).
const MAX_PAGE_BYTES: usize = 8 * 1024 * 1024;

#[derive(Debug, Clone)]
pub struct BalldontlieConfig {
    pub api_key: String,
    pub base_url: String,
    pub requests_per_minute: u32,
}

#[derive(Debug)]
pub struct BalldontlieSource {
    client: reqwest::Client,
    config: BalldontlieConfig,
    /// Earliest instant the next request may start (pacing for the per-minute quota).
    next_slot: Mutex<Instant>,
    /// The source's team identifiers by dataset code, fetched once for the injury report.
    teams: Mutex<Option<HashMap<i64, String>>>,
}

impl BalldontlieSource {
    pub fn new(config: BalldontlieConfig) -> Result<Self, SourceError> {
        if config.api_key.trim().is_empty() {
            return Err(SourceError::NotConfigured(
                "BALLDONTLIE_API_KEY is empty".to_string(),
            ));
        }
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(60))
            .user_agent(concat!("nba-elo-simulator/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|e| SourceError::Transport(e.to_string()))?;
        Ok(Self {
            client,
            config,
            next_slot: Mutex::new(Instant::now()),
            teams: Mutex::new(None),
        })
    }

    pub fn base_url(&self) -> &str {
        &self.config.base_url
    }

    pub fn requests_per_minute(&self) -> u32 {
        self.config.requests_per_minute
    }

    fn interval(&self) -> Duration {
        let per_minute = self.config.requests_per_minute.max(1);
        Duration::from_millis(60_000 / u64::from(per_minute))
    }

    /// Wait for the next request slot, then reserve the one after it.
    async fn pace(&self, progress: &dyn FetchProgressSink, so_far: FetchProgress) {
        let mut slot = self.next_slot.lock().await;
        let now = Instant::now();
        if *slot > now {
            progress.report(FetchProgress {
                waiting: Some(*slot - now),
                rate_limited: false,
                ..so_far
            });
            tokio::time::sleep_until(*slot).await;
        }
        *slot = Instant::now() + self.interval();
    }

    async fn fetch_page(
        &self,
        season: SeasonYear,
        window: FetchWindow,
        cursor: Option<i64>,
        progress: &dyn FetchProgressSink,
        so_far: FetchProgress,
    ) -> Result<Vec<u8>, SourceError> {
        let url = format!("{}/games", self.config.base_url.trim_end_matches('/'));
        let mut retries = 0;
        loop {
            self.pace(progress, so_far).await;
            let req = self
                .client
                .get(&url)
                .header("Authorization", self.config.api_key.as_str())
                .query(&query_params(season, window, cursor));
            let resp = req
                .send()
                .await
                .map_err(|e| SourceError::Transport(e.to_string()))?;
            let status = resp.status();
            if status.as_u16() == 429 {
                retries += 1;
                if retries > MAX_RATE_LIMIT_RETRIES {
                    return Err(SourceError::RateLimited(MAX_RATE_LIMIT_RETRIES));
                }
                let wait = resp
                    .headers()
                    .get("retry-after")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| v.parse::<u64>().ok())
                    .map(Duration::from_secs)
                    .unwrap_or_else(|| self.interval() * 2);
                tracing::warn!(?wait, retries, "balldontlie rate limit hit; waiting");
                progress.report(FetchProgress {
                    waiting: Some(wait),
                    rate_limited: true,
                    ..so_far
                });
                tokio::time::sleep(wait).await;
                continue;
            }
            if !status.is_success() {
                let message = resp.text().await.unwrap_or_default();
                return Err(SourceError::Rejected {
                    status: status.as_u16(),
                    message: message.chars().take(300).collect(),
                });
            }
            if let Some(len) = resp.content_length() {
                if len as usize > MAX_PAGE_BYTES {
                    return Err(SourceError::InvalidPayload(format!("page of {len} bytes")));
                }
            }
            let bytes = resp
                .bytes()
                .await
                .map_err(|e| SourceError::Transport(e.to_string()))?;
            if bytes.len() > MAX_PAGE_BYTES {
                return Err(SourceError::InvalidPayload(format!(
                    "page of {} bytes",
                    bytes.len()
                )));
            }
            return Ok(bytes.to_vec());
        }
    }
}

impl ResultsSource for BalldontlieSource {
    fn name(&self) -> &'static str {
        "balldontlie"
    }

    async fn season_results(
        &self,
        season: SeasonYear,
        window: FetchWindow,
        progress: &dyn FetchProgressSink,
    ) -> Result<ResultsFetch, SourceError> {
        let query: Vec<String> = query_params(season, window, None)
            .into_iter()
            .filter(|(k, _)| *k != "per_page")
            .map(|(k, v)| format!("{k}={v}"))
            .collect();
        let mut fetch = ResultsFetch {
            source_url: format!(
                "{}/games?{}",
                self.config.base_url.trim_end_matches('/'),
                query.join("&")
            ),
            ..Default::default()
        };
        let mut cursor = None;
        for _ in 0..MAX_PAGES {
            let so_far = FetchProgress {
                requests: fetch.requests,
                games: fetch.records.len(),
                ..Default::default()
            };
            let body = self
                .fetch_page(season, window, cursor, progress, so_far)
                .await?;
            fetch.requests += 1;
            let page = parse_games_page(&body)?;
            fetch.records.extend(page.records);
            progress.report(FetchProgress {
                requests: fetch.requests,
                games: fetch.records.len(),
                ..Default::default()
            });
            match page.next_cursor {
                Some(c) => cursor = Some(c),
                None => return Ok(fetch),
            }
        }
        Err(SourceError::InvalidPayload(format!(
            "more than {MAX_PAGES} pages for one season"
        )))
    }
}

// ---------------------------------------------------------------------------------------
// Request and payload (pure; unit tested)
// ---------------------------------------------------------------------------------------

/// Query string of one page: the season (by its start year), the page size, the window
/// bounds when set, and the cursor after the first page.
fn query_params(
    season: SeasonYear,
    window: FetchWindow,
    cursor: Option<i64>,
) -> Vec<(&'static str, String)> {
    let mut q = vec![
        ("seasons[]", (season.end_year() - 1).to_string()),
        ("per_page", PER_PAGE.to_string()),
    ];
    if let Some(from) = window.from {
        q.push(("start_date", from.to_string()));
    }
    if let Some(to) = window.to {
        q.push(("end_date", to.to_string()));
    }
    if let Some(c) = cursor {
        q.push(("cursor", c.to_string()));
    }
    q
}

#[derive(Debug, Deserialize)]
struct GamesPage {
    data: Vec<ApiGame>,
    #[serde(default)]
    meta: Meta,
}

#[derive(Debug, Default, Deserialize)]
struct Meta {
    #[serde(default)]
    next_cursor: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct ApiGame {
    date: String,
    #[serde(default)]
    status: String,
    #[serde(default)]
    postseason: bool,
    #[serde(default)]
    home_team_score: i32,
    #[serde(default)]
    visitor_team_score: i32,
    home_team: ApiTeam,
    visitor_team: ApiTeam,
}

#[derive(Debug, Deserialize)]
struct ApiTeam {
    abbreviation: String,
}

#[derive(Debug, Default, PartialEq)]
pub struct ParsedPage {
    pub records: Vec<ResultRecord>,
    pub next_cursor: Option<i64>,
}

/// Map a BALLDONTLIE abbreviation to the Elo dataset's team code.
pub fn dataset_code(abbreviation: &str) -> String {
    match abbreviation.trim().to_ascii_uppercase().as_str() {
        "BKN" => "BRK".to_string(),
        "CHA" => "CHO".to_string(),
        "PHX" => "PHO".to_string(),
        other => other.to_string(),
    }
}

fn parse_date(s: &str) -> Option<NaiveDate> {
    let day = s.get(..10)?;
    NaiveDate::parse_from_str(day, "%Y-%m-%d").ok()
}

/// Parse one page of `GET /games`. A game counts as played only when its status is `Final`;
/// every other game becomes a scheduled game without scores.
pub fn parse_games_page(body: &[u8]) -> Result<ParsedPage, SourceError> {
    let page: GamesPage =
        serde_json::from_slice(body).map_err(|e| SourceError::InvalidPayload(e.to_string()))?;
    let mut records = Vec::with_capacity(page.data.len());
    for g in page.data {
        let date = parse_date(&g.date)
            .ok_or_else(|| SourceError::InvalidPayload(format!("bad game date {:?}", g.date)))?;
        let played = g.status.trim().eq_ignore_ascii_case("final");
        records.push(ResultRecord {
            date,
            home_code: dataset_code(&g.home_team.abbreviation),
            away_code: dataset_code(&g.visitor_team.abbreviation),
            neutral: false,
            playoff: g.postseason,
            home_score: played.then_some(g.home_team_score),
            away_score: played.then_some(g.visitor_team_score),
        });
    }
    Ok(ParsedPage {
        records,
        next_cursor: page.meta.next_cursor,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_names_the_season_start_year_and_the_window_bounds() {
        let season = SeasonYear::of(2027);
        let all = query_params(season, FetchWindow::all(), None);
        assert_eq!(
            all,
            vec![
                ("seasons[]", "2026".to_string()),
                ("per_page", "100".to_string())
            ]
        );
        let d = |s: &str| NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap();
        let page2 = query_params(
            season,
            FetchWindow::between(d("2026-10-21"), d("2026-10-23")),
            Some(7),
        );
        assert_eq!(
            page2[2..],
            [
                ("start_date", "2026-10-21".to_string()),
                ("end_date", "2026-10-23".to_string()),
                ("cursor", "7".to_string())
            ]
        );
    }

    const PAGE: &str = r#"{"data":[
      {"id":1,"date":"2025-10-21","season":2025,"status":"Final","period":4,"time":"Final","postseason":false,
       "home_team_score":125,"visitor_team_score":124,
       "home_team":{"id":21,"abbreviation":"OKC","full_name":"Oklahoma City Thunder"},
       "visitor_team":{"id":11,"abbreviation":"HOU","full_name":"Houston Rockets"}},
      {"id":2,"date":"2025-10-22T00:00:00.000Z","season":2025,"status":"2025-10-22T02:00:00Z","period":0,"time":"","postseason":false,
       "home_team_score":0,"visitor_team_score":0,
       "home_team":{"id":3,"abbreviation":"BKN"},"visitor_team":{"id":4,"abbreviation":"CHA"}},
      {"id":3,"date":"2026-04-20","season":2025,"status":"Final","postseason":true,
       "home_team_score":101,"visitor_team_score":99,
       "home_team":{"id":24,"abbreviation":"PHX"},"visitor_team":{"id":14,"abbreviation":"LAL"}}
    ],"meta":{"next_cursor":3,"per_page":100}}"#;

    #[test]
    fn parses_played_scheduled_and_postseason_games() {
        let page = parse_games_page(PAGE.as_bytes()).unwrap();
        assert_eq!(page.next_cursor, Some(3));
        assert_eq!(page.records.len(), 3);
        let g = &page.records[0];
        assert_eq!(g.date, NaiveDate::from_ymd_opt(2025, 10, 21).unwrap());
        assert_eq!((g.home_code.as_str(), g.away_code.as_str()), ("OKC", "HOU"));
        assert_eq!((g.home_score, g.away_score), (Some(125), Some(124)));
        assert!(!g.playoff);
        let s = &page.records[1];
        assert_eq!(s.date, NaiveDate::from_ymd_opt(2025, 10, 22).unwrap());
        assert_eq!((s.home_code.as_str(), s.away_code.as_str()), ("BRK", "CHO"));
        assert_eq!((s.home_score, s.away_score), (None, None));
        let p = &page.records[2];
        assert_eq!(p.home_code, "PHO");
        assert!(p.playoff);
    }

    #[test]
    fn last_page_has_no_cursor() {
        let page = parse_games_page(br#"{"data":[],"meta":{"per_page":100}}"#).unwrap();
        assert_eq!(page, ParsedPage::default());
        assert!(parse_games_page(b"not json").is_err());
        assert!(parse_games_page(br#"{"data":[{"date":"soon","home_team":{"abbreviation":"A"},"visitor_team":{"abbreviation":"B"}}]}"#).is_err());
    }

    #[test]
    fn rejects_empty_key() {
        let err = BalldontlieSource::new(BalldontlieConfig {
            api_key: "  ".into(),
            base_url: "https://api.balldontlie.io/v1".into(),
            requests_per_minute: 5,
        })
        .err()
        .unwrap();
        assert!(matches!(err, SourceError::NotConfigured(_)));
    }
}

// ---------------------------------------------------------------------------------------
// Player source: box scores by date and the injury report
// ---------------------------------------------------------------------------------------

/// The injury report has no season; the sync asks for it once a day.
const INJURIES_PATH: &str = "player_injuries";
const STATS_PATH: &str = "stats";
const TEAMS_PATH: &str = "teams";

impl BalldontlieSource {
    /// One request to `path` with `query`, paced and retried like a games page.
    async fn get(
        &self,
        path: &str,
        query: &[(&str, String)],
        progress: &dyn FetchProgressSink,
        so_far: FetchProgress,
    ) -> Result<Vec<u8>, SourceError> {
        let url = format!("{}/{path}", self.config.base_url.trim_end_matches('/'));
        let mut retries = 0;
        loop {
            self.pace(progress, so_far).await;
            let resp = self
                .client
                .get(&url)
                .header("Authorization", self.config.api_key.as_str())
                .query(query)
                .send()
                .await
                .map_err(|e| SourceError::Transport(e.to_string()))?;
            let status = resp.status();
            if status.as_u16() == 429 {
                retries += 1;
                if retries > MAX_RATE_LIMIT_RETRIES {
                    return Err(SourceError::RateLimited(MAX_RATE_LIMIT_RETRIES));
                }
                let wait = resp
                    .headers()
                    .get("retry-after")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| v.parse::<u64>().ok())
                    .map(Duration::from_secs)
                    .unwrap_or_else(|| self.interval() * 2);
                tracing::warn!(?wait, retries, path, "balldontlie rate limit hit; waiting");
                progress.report(FetchProgress {
                    waiting: Some(wait),
                    rate_limited: true,
                    ..so_far
                });
                tokio::time::sleep(wait).await;
                continue;
            }
            if !status.is_success() {
                let message = resp.text().await.unwrap_or_default();
                return Err(SourceError::Rejected {
                    status: status.as_u16(),
                    message: message.chars().take(300).collect(),
                });
            }
            let bytes = resp
                .bytes()
                .await
                .map_err(|e| SourceError::Transport(e.to_string()))?;
            if bytes.len() > MAX_PAGE_BYTES {
                return Err(SourceError::InvalidPayload(format!(
                    "page of {} bytes",
                    bytes.len()
                )));
            }
            return Ok(bytes.to_vec());
        }
    }

    /// The source's team identifiers by abbreviation, fetched once.
    async fn team_codes(
        &self,
        progress: &dyn FetchProgressSink,
    ) -> Result<HashMap<i64, String>, SourceError> {
        if let Some(map) = self.teams.lock().await.as_ref() {
            return Ok(map.clone());
        }
        let body = self
            .get(TEAMS_PATH, &[], progress, FetchProgress::default())
            .await?;
        let map = parse_teams_page(&body)?;
        *self.teams.lock().await = Some(map.clone());
        Ok(map)
    }
}

impl PlayerSource for BalldontlieSource {
    fn name(&self) -> &'static str {
        "balldontlie"
    }

    async fn box_scores_on(
        &self,
        date: NaiveDate,
        progress: &dyn FetchProgressSink,
    ) -> Result<Vec<BoxScoreLine>, SourceError> {
        let mut lines = Vec::new();
        let mut cursor: Option<i64> = None;
        for requests in 0..MAX_PAGES {
            let mut query = vec![
                ("dates[]", date.to_string()),
                ("per_page", PER_PAGE.to_string()),
            ];
            if let Some(c) = cursor {
                query.push(("cursor", c.to_string()));
            }
            let so_far = FetchProgress {
                requests,
                games: lines.len(),
                ..Default::default()
            };
            let body = self.get(STATS_PATH, &query, progress, so_far).await?;
            let page = parse_stats_page(&body)?;
            lines.extend(page.lines);
            progress.report(FetchProgress {
                requests: requests + 1,
                games: lines.len(),
                ..Default::default()
            });
            match page.next_cursor {
                Some(c) => cursor = Some(c),
                None => return Ok(lines),
            }
        }
        Err(SourceError::InvalidPayload(format!(
            "more than {MAX_PAGES} pages of box scores for one date"
        )))
    }

    async fn injury_report(
        &self,
        progress: &dyn FetchProgressSink,
    ) -> Result<Vec<InjuryRecord>, SourceError> {
        let teams = self.team_codes(progress).await?;
        let mut records = Vec::new();
        let mut cursor: Option<i64> = None;
        for requests in 0..MAX_PAGES {
            let mut query = vec![("per_page", PER_PAGE.to_string())];
            if let Some(c) = cursor {
                query.push(("cursor", c.to_string()));
            }
            let so_far = FetchProgress {
                requests,
                games: records.len(),
                ..Default::default()
            };
            let body = self.get(INJURIES_PATH, &query, progress, so_far).await?;
            let page = parse_injuries_page(&body, &teams)?;
            records.extend(page.records);
            progress.report(FetchProgress {
                requests: requests + 1,
                games: records.len(),
                ..Default::default()
            });
            match page.next_cursor {
                Some(c) => cursor = Some(c),
                None => return Ok(records),
            }
        }
        Err(SourceError::InvalidPayload(format!(
            "more than {MAX_PAGES} pages of injuries"
        )))
    }
}

#[derive(Debug, Deserialize)]
struct ApiPlayer {
    id: i64,
    #[serde(default)]
    first_name: String,
    #[serde(default)]
    last_name: String,
    #[serde(default)]
    position: Option<String>,
    #[serde(default)]
    height: Option<String>,
    #[serde(default)]
    weight: Option<String>,
    #[serde(default)]
    college: Option<String>,
    #[serde(default)]
    country: Option<String>,
    #[serde(default)]
    draft_year: Option<i32>,
    #[serde(default)]
    draft_round: Option<i32>,
    #[serde(default)]
    draft_number: Option<i32>,
    #[serde(default)]
    team_id: Option<i64>,
}

impl ApiPlayer {
    fn record(&self) -> PlayerRecord {
        let clean = |s: &Option<String>| {
            s.as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(String::from)
        };
        PlayerRecord {
            source_id: self.id,
            first_name: self.first_name.trim().to_string(),
            last_name: self.last_name.trim().to_string(),
            position: clean(&self.position),
            height: clean(&self.height),
            weight: clean(&self.weight),
            college: clean(&self.college),
            country: clean(&self.country),
            draft_year: self.draft_year,
            draft_round: self.draft_round,
            draft_number: self.draft_number,
        }
    }
}

#[derive(Debug, Deserialize)]
struct ApiStatTeam {
    abbreviation: String,
}

#[derive(Debug, Deserialize)]
struct ApiStatGame {
    date: String,
}

#[derive(Debug, Deserialize)]
struct ApiStat {
    #[serde(default)]
    min: Option<String>,
    #[serde(default)]
    pts: Option<i32>,
    #[serde(default)]
    reb: Option<i32>,
    #[serde(default)]
    oreb: Option<i32>,
    #[serde(default)]
    dreb: Option<i32>,
    #[serde(default)]
    ast: Option<i32>,
    #[serde(default)]
    stl: Option<i32>,
    #[serde(default)]
    blk: Option<i32>,
    #[serde(default)]
    turnover: Option<i32>,
    #[serde(default)]
    pf: Option<i32>,
    #[serde(default)]
    fgm: Option<i32>,
    #[serde(default)]
    fga: Option<i32>,
    #[serde(default)]
    fg3m: Option<i32>,
    #[serde(default)]
    fg3a: Option<i32>,
    #[serde(default)]
    ftm: Option<i32>,
    #[serde(default)]
    fta: Option<i32>,
    player: ApiPlayer,
    team: ApiStatTeam,
    game: ApiStatGame,
}

#[derive(Debug, Deserialize)]
struct StatsPage {
    data: Vec<ApiStat>,
    #[serde(default)]
    meta: Meta,
}

#[derive(Debug, Default, PartialEq)]
pub struct ParsedStats {
    pub lines: Vec<BoxScoreLine>,
    pub next_cursor: Option<i64>,
}

/// Playing time as the source writes it (`"34:12"`, `"34"`, `""`) in seconds; `None` when
/// absent or unreadable.
pub fn parse_minutes(min: Option<&str>) -> Option<i32> {
    let s = min?.trim();
    if s.is_empty() {
        return None;
    }
    let mut parts = s.splitn(2, ':');
    let minutes: i32 = parts.next()?.trim().parse().ok()?;
    let seconds: i32 = match parts.next() {
        Some(sec) => sec.trim().parse().ok()?,
        None => 0,
    };
    (minutes >= 0 && (0..60).contains(&seconds)).then_some(minutes * 60 + seconds)
}

/// Parse one page of `GET /stats`: one line per player, the game named by its date and
/// the player's team.
pub fn parse_stats_page(body: &[u8]) -> Result<ParsedStats, SourceError> {
    let page: StatsPage =
        serde_json::from_slice(body).map_err(|e| SourceError::InvalidPayload(e.to_string()))?;
    let mut lines = Vec::with_capacity(page.data.len());
    for s in page.data {
        let date = parse_date(&s.game.date).ok_or_else(|| {
            SourceError::InvalidPayload(format!("bad game date {:?}", s.game.date))
        })?;
        lines.push(BoxScoreLine {
            date,
            team_code: dataset_code(&s.team.abbreviation),
            player: s.player.record(),
            seconds: parse_minutes(s.min.as_deref()),
            pts: s.pts,
            reb: s.reb,
            oreb: s.oreb,
            dreb: s.dreb,
            ast: s.ast,
            stl: s.stl,
            blk: s.blk,
            tov: s.turnover,
            pf: s.pf,
            fgm: s.fgm,
            fga: s.fga,
            fg3m: s.fg3m,
            fg3a: s.fg3a,
            ftm: s.ftm,
            fta: s.fta,
        });
    }
    Ok(ParsedStats {
        lines,
        next_cursor: page.meta.next_cursor,
    })
}

#[derive(Debug, Deserialize)]
struct TeamsPage {
    data: Vec<ApiTeamRow>,
}

#[derive(Debug, Deserialize)]
struct ApiTeamRow {
    id: i64,
    abbreviation: String,
}

/// Parse `GET /teams` into the dataset code of each source team identifier.
pub fn parse_teams_page(body: &[u8]) -> Result<HashMap<i64, String>, SourceError> {
    let page: TeamsPage =
        serde_json::from_slice(body).map_err(|e| SourceError::InvalidPayload(e.to_string()))?;
    Ok(page
        .data
        .into_iter()
        .map(|t| (t.id, dataset_code(&t.abbreviation)))
        .collect())
}

#[derive(Debug, Deserialize)]
struct ApiInjury {
    player: ApiPlayer,
    #[serde(default)]
    return_date: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    status: Option<String>,
}

#[derive(Debug, Deserialize)]
struct InjuriesPage {
    data: Vec<ApiInjury>,
    #[serde(default)]
    meta: Meta,
}

#[derive(Debug, Default, PartialEq)]
pub struct ParsedInjuries {
    pub records: Vec<InjuryRecord>,
    pub next_cursor: Option<i64>,
}

/// Parse one page of `GET /player_injuries`. A line whose player has no team the source
/// names is dropped: the report is by team.
pub fn parse_injuries_page(
    body: &[u8],
    teams: &HashMap<i64, String>,
) -> Result<ParsedInjuries, SourceError> {
    let page: InjuriesPage =
        serde_json::from_slice(body).map_err(|e| SourceError::InvalidPayload(e.to_string()))?;
    let mut records = Vec::with_capacity(page.data.len());
    for i in page.data {
        let Some(team_code) = i.player.team_id.and_then(|id| teams.get(&id)).cloned() else {
            continue;
        };
        records.push(InjuryRecord {
            player: i.player.record(),
            team_code,
            status: i
                .status
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .unwrap_or("Unknown")
                .to_string(),
            return_date: i.return_date.as_deref().and_then(parse_date),
            description: i
                .description
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(|s| s.chars().take(500).collect()),
        });
    }
    Ok(ParsedInjuries {
        records,
        next_cursor: page.meta.next_cursor,
    })
}

#[cfg(test)]
mod player_tests {
    use super::*;

    #[test]
    fn minutes_parse_in_both_forms() {
        assert_eq!(parse_minutes(Some("34:12")), Some(2052));
        assert_eq!(parse_minutes(Some("34")), Some(2040));
        assert_eq!(parse_minutes(Some("0:00")), Some(0));
        assert_eq!(parse_minutes(Some("")), None);
        assert_eq!(parse_minutes(Some("34:75")), None);
        assert_eq!(parse_minutes(Some("abc")), None);
        assert_eq!(parse_minutes(None), None);
    }

    const STATS: &str = r#"{"data":[
      {"id":1,"min":"34:12","fgm":9,"fga":20,"fg3m":5,"fg3a":12,"ftm":7,"fta":8,"oreb":1,"dreb":4,"reb":5,"ast":7,"stl":1,"blk":0,"turnover":3,"pf":2,"pts":30,
       "player":{"id":115,"first_name":"Stephen","last_name":"Curry","position":"G","height":"6-2","weight":"185","jersey_number":"30","college":"Davidson","country":"USA","draft_year":2009,"draft_round":1,"draft_number":7,"team_id":10},
       "team":{"id":10,"abbreviation":"GSW","full_name":"Golden State Warriors"},
       "game":{"id":5,"date":"2019-06-13","season":2018,"status":"Final","home_team_id":10,"visitor_team_id":28,"postseason":true}},
      {"id":2,"min":"","pts":0,
       "player":{"id":200,"first_name":"Kevin","last_name":"Durant","team_id":10},
       "team":{"id":10,"abbreviation":"GSW"},
       "game":{"id":5,"date":"2019-06-13T00:00:00.000Z","home_team_id":10}},
      {"id":3,"min":"40:01","pts":26,
       "player":{"id":300,"first_name":"Kawhi","last_name":"Leonard","position":"","team_id":28},
       "team":{"id":28,"abbreviation":"TOR"},
       "game":{"id":5,"date":"2019-06-13","home_team_id":10}}
    ],"meta":{"next_cursor":3,"per_page":100}}"#;

    #[test]
    fn parses_box_score_lines_with_the_game_by_date_and_team() {
        let page = parse_stats_page(STATS.as_bytes()).unwrap();
        assert_eq!(page.next_cursor, Some(3));
        assert_eq!(page.lines.len(), 3);
        let c = &page.lines[0];
        assert_eq!(c.date, NaiveDate::from_ymd_opt(2019, 6, 13).unwrap());
        assert_eq!(c.team_code, "GSW");
        assert_eq!(c.player.source_id, 115);
        assert_eq!(c.player.last_name, "Curry");
        assert_eq!(c.player.draft_number, Some(7));
        assert_eq!(
            (c.seconds, c.pts, c.tov, c.fg3m),
            (Some(2052), Some(30), Some(3), Some(5))
        );
        let d = &page.lines[1];
        assert_eq!((d.seconds, d.pts, d.reb), (None, Some(0), None));
        let k = &page.lines[2];
        assert_eq!(k.team_code, "TOR");
        assert_eq!(k.player.position, None);
        assert!(parse_stats_page(b"[]").is_err());
    }

    #[test]
    fn parses_the_injury_report_by_team() {
        let teams = parse_teams_page(
            br#"{"data":[{"id":10,"abbreviation":"GSW"},{"id":4,"abbreviation":"CHA"}]}"#,
        )
        .unwrap();
        assert_eq!(teams[&4], "CHO");
        let page = parse_injuries_page(
            br#"{"data":[
              {"player":{"id":115,"first_name":"Stephen","last_name":"Curry","team_id":10},"return_date":"2026-11-02","description":"Right knee; out","status":"Out"},
              {"player":{"id":7,"first_name":"Nobody","last_name":"Traded","team_id":99},"status":"Day-To-Day"},
              {"player":{"id":8,"first_name":"Blank","last_name":"Status","team_id":4},"status":"  "}
            ],"meta":{"per_page":100}}"#,
            &teams,
        )
        .unwrap();
        assert_eq!(page.next_cursor, None);
        assert_eq!(page.records.len(), 2);
        let c = &page.records[0];
        assert_eq!((c.team_code.as_str(), c.status.as_str()), ("GSW", "Out"));
        assert_eq!(c.return_date, NaiveDate::from_ymd_opt(2026, 11, 2));
        assert_eq!(c.description.as_deref(), Some("Right knee; out"));
        assert_eq!(
            (
                page.records[1].team_code.as_str(),
                page.records[1].status.as_str()
            ),
            ("CHO", "Unknown")
        );
    }
}
