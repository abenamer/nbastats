//! Outbound adapter: game results and schedule from the BALLDONTLIE NBA API (v1).
//!
//! `GET /v1/games?seasons[]=<start year>&per_page=100&cursor=<n>` pages through a season.
//! The free tier allows 5 requests per minute, so requests are paced and a 429 is retried
//! after the `Retry-After` delay. Team abbreviations differ from the Elo dataset for three
//! teams (BKN, CHA, PHX); `dataset_code` maps them.

use std::time::Duration;

use chrono::NaiveDate;
use serde::Deserialize;
use tokio::sync::Mutex;
use tokio::time::Instant;

use crate::application::ports::{
    FetchProgress, FetchProgressSink, ResultRecord, ResultsFetch, ResultsSource, SourceError,
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
        cursor: Option<i64>,
        progress: &dyn FetchProgressSink,
        so_far: FetchProgress,
    ) -> Result<Vec<u8>, SourceError> {
        let url = format!("{}/games", self.config.base_url.trim_end_matches('/'));
        let start_year = (season.end_year() - 1).to_string();
        let mut retries = 0;
        loop {
            self.pace(progress, so_far).await;
            let mut req = self
                .client
                .get(&url)
                .header("Authorization", self.config.api_key.as_str())
                .query(&[
                    ("seasons[]", start_year.as_str()),
                    ("per_page", &PER_PAGE.to_string()),
                ]);
            if let Some(c) = cursor {
                req = req.query(&[("cursor", c.to_string())]);
            }
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
        progress: &dyn FetchProgressSink,
    ) -> Result<ResultsFetch, SourceError> {
        let mut fetch = ResultsFetch {
            source_url: format!(
                "{}/games?seasons[]={}",
                self.config.base_url.trim_end_matches('/'),
                season.end_year() - 1
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
            let body = self.fetch_page(season, cursor, progress, so_far).await?;
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
// Payload parsing (pure; unit tested)
// ---------------------------------------------------------------------------------------

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
