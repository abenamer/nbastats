//! Outbound adapter: game reports from ESPN's public scoreboard feed. For a date, the feed
//! lists each game with the teams and, when ESPN published one, a link to the game report
//! (the Associated Press recap). One request per date, a 20-second timeout, a 4 MB cap on
//! the body, no redirects. Only `https` links on an ESPN host are kept.

use std::time::Duration;

use chrono::NaiveDate;
use serde_json::Value;

use crate::application::ports::{ReportCandidate, ReportListing, ReportSource, SourceError};

const MAX_BODY_BYTES: usize = 4 * 1024 * 1024;
const MAX_TITLE_CHARS: usize = 200;

#[derive(Debug)]
pub struct EspnReportSource {
    client: reqwest::Client,
    base_url: String,
}

impl EspnReportSource {
    pub fn new(base_url: String) -> Result<Self, SourceError> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(20))
            .redirect(reqwest::redirect::Policy::none())
            .user_agent(concat!("nba-elo-simulator/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|e| SourceError::Transport(e.to_string()))?;
        Ok(Self {
            client,
            base_url: base_url.trim_end_matches('/').to_string(),
        })
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }
}

/// Whether a report link may be stored: `https`, a plain host name (letters, digits, dots
/// and dashes only, so no credentials, port or backslash tricks) on an ESPN domain.
pub fn acceptable_url(url: &str) -> bool {
    let Some(rest) = url.strip_prefix("https://") else {
        return false;
    };
    let host = rest.split(['/', '?', '#']).next().unwrap_or("");
    let plain = !host.is_empty()
        && host
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-');
    plain && (host == "espn.com" || host.ends_with(".espn.com"))
}

fn text(v: &Value, key: &str) -> Option<String> {
    v.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

/// The games and reports in one scoreboard answer. A game without a report link is counted
/// but not listed.
pub fn parse_scoreboard(bytes: &[u8]) -> Result<ReportListing, SourceError> {
    let doc: Value = serde_json::from_slice(bytes)
        .map_err(|e| SourceError::InvalidPayload(format!("not a scoreboard: {e}")))?;
    let events = doc
        .get("events")
        .and_then(Value::as_array)
        .ok_or_else(|| SourceError::InvalidPayload("no events".to_string()))?;
    let mut out = ReportListing {
        games_listed: events.len(),
        ..Default::default()
    };
    for event in events {
        let Some(competition) = event
            .get("competitions")
            .and_then(Value::as_array)
            .and_then(|c| c.first())
        else {
            continue;
        };
        let competitors = competition
            .get("competitors")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let side = |which: &str| -> Option<(String, String)> {
            let c = competitors
                .iter()
                .find(|c| c.get("homeAway").and_then(Value::as_str) == Some(which))?;
            let team = c.get("team")?;
            Some((
                text(team, "abbreviation")?.to_ascii_uppercase(),
                text(team, "displayName")?,
            ))
        };
        let (Some((home_abbr, home_name)), Some((away_abbr, away_name))) =
            (side("home"), side("away"))
        else {
            continue;
        };
        // The report link: a link tagged "recap", else the recap page of the event when the
        // feed carries a recap headline.
        let tagged = event
            .get("links")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .find(|l| {
                l.get("rel")
                    .and_then(Value::as_array)
                    .is_some_and(|rel| rel.iter().any(|r| r.as_str() == Some("recap")))
            })
            .and_then(|l| text(l, "href"));
        let headline = competition
            .get("headlines")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .find(|h| {
                h.get("type")
                    .and_then(Value::as_str)
                    .is_some_and(|t| t.eq_ignore_ascii_case("recap"))
            });
        let url = tagged.or_else(|| {
            headline?;
            text(event, "id").map(|id| format!("https://www.espn.com/nba/recap/_/gameId/{id}"))
        });
        let Some(url) = url.filter(|u| acceptable_url(u)) else {
            continue;
        };
        let title = headline
            .and_then(|h| text(h, "shortLinkText").or_else(|| text(h, "description")))
            .map(|t| t.chars().take(MAX_TITLE_CHARS).collect());
        out.reports.push(ReportCandidate {
            home_abbr,
            away_abbr,
            home_name,
            away_name,
            url,
            title,
        });
    }
    Ok(out)
}

impl ReportSource for EspnReportSource {
    fn name(&self) -> &'static str {
        "espn"
    }

    async fn reports_on(&self, date: NaiveDate) -> Result<ReportListing, SourceError> {
        let url = format!(
            "{}/apis/site/v2/sports/basketball/nba/scoreboard",
            self.base_url
        );
        let resp = self
            .client
            .get(&url)
            .query(&[
                ("dates", date.format("%Y%m%d").to_string()),
                ("limit", "100".into()),
            ])
            .send()
            .await
            .map_err(|e| SourceError::Transport(e.to_string()))?;
        let status = resp.status();
        let mut resp = resp;
        let mut bytes: Vec<u8> = Vec::new();
        while let Some(chunk) = resp
            .chunk()
            .await
            .map_err(|e| SourceError::Transport(e.to_string()))?
        {
            bytes.extend_from_slice(&chunk);
            if bytes.len() > MAX_BODY_BYTES {
                return Err(SourceError::InvalidPayload(format!(
                    "answer longer than {MAX_BODY_BYTES} bytes"
                )));
            }
        }
        if !status.is_success() {
            return Err(SourceError::Rejected {
                status: status.as_u16(),
                message: String::from_utf8_lossy(&bytes).chars().take(300).collect(),
            });
        }
        parse_scoreboard(&bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"{"events":[
      {"id":"401161489","name":"Dallas Mavericks at Golden State Warriors",
       "competitions":[{"competitors":[
          {"homeAway":"home","team":{"abbreviation":"GS","displayName":"Golden State Warriors"}},
          {"homeAway":"away","team":{"abbreviation":"DAL","displayName":"Dallas Mavericks"}}],
         "headlines":[{"type":"Recap","shortLinkText":"Doncic's triple-double, Nowitzki's 21 lead Mavs past Warriors 126-91"}]}],
       "links":[{"rel":["summary","desktop","event"],"href":"https://www.espn.com/nba/game/_/gameId/401161489"},
                {"rel":["recap","desktop","event"],"href":"https://www.espn.com/nba/recap/_/gameId/401161489"}]},
      {"id":"401161490","competitions":[{"competitors":[
          {"homeAway":"home","team":{"abbreviation":"SA","displayName":"San Antonio Spurs"}},
          {"homeAway":"away","team":{"abbreviation":"NY","displayName":"New York Knicks"}}],
         "headlines":[{"type":"Recap","description":"Spurs top Knicks"}]}],
       "links":[{"rel":["summary"],"href":"https://www.espn.com/nba/game/_/gameId/401161490"}]},
      {"id":"401161491","competitions":[{"competitors":[
          {"homeAway":"home","team":{"abbreviation":"LAL","displayName":"Los Angeles Lakers"}},
          {"homeAway":"away","team":{"abbreviation":"BOS","displayName":"Boston Celtics"}}]}],
       "links":[{"rel":["summary"],"href":"https://www.espn.com/nba/game/_/gameId/401161491"}]},
      {"id":"9","competitions":[{"competitors":[
          {"homeAway":"home","team":{"abbreviation":"MIA","displayName":"Miami Heat"}},
          {"homeAway":"away","team":{"abbreviation":"ORL","displayName":"Orlando Magic"}}],
         "headlines":[{"type":"Recap","shortLinkText":"x"}]}],
       "links":[{"rel":["recap"],"href":"http://evil.example/recap"}]}
    ]}"#;

    #[test]
    fn reports_are_read_with_their_teams_and_titles() {
        let listing = parse_scoreboard(SAMPLE.as_bytes()).unwrap();
        assert_eq!(listing.games_listed, 4);
        let r = listing.reports;
        // The Lakers game has no report; the fourth game's link is not an ESPN https link.
        assert_eq!(r.len(), 2);
        assert_eq!(
            (r[0].home_abbr.as_str(), r[0].away_abbr.as_str()),
            ("GS", "DAL")
        );
        assert_eq!(r[0].home_name, "Golden State Warriors");
        assert_eq!(
            r[0].url,
            "https://www.espn.com/nba/recap/_/gameId/401161489"
        );
        assert!(r[0].title.as_deref().unwrap().starts_with("Doncic"));
        // Without a tagged link, a recap headline gives the recap page of the event.
        assert_eq!(
            r[1].url,
            "https://www.espn.com/nba/recap/_/gameId/401161490"
        );
        assert_eq!(r[1].title.as_deref(), Some("Spurs top Knicks"));
    }

    #[test]
    fn only_https_espn_links_are_kept() {
        assert!(acceptable_url("https://www.espn.com/nba/recap/_/gameId/1"));
        assert!(acceptable_url("https://espn.com/x"));
        assert!(!acceptable_url("http://www.espn.com/nba/recap/_/gameId/1"));
        assert!(!acceptable_url("https://www.espn.com.evil.example/x"));
        assert!(!acceptable_url("https://evil.example/?u=espn.com"));
        assert!(!acceptable_url("https://user@evil.example/espn.com"));
        assert!(!acceptable_url("https://evil.example\\@www.espn.com/x"));
        assert!(!acceptable_url("https://www.espn.com:8443/x"));
        assert!(!acceptable_url("https://www.espn.com@evil.example/x"));
        assert!(matches!(
            parse_scoreboard(b"<html>"),
            Err(SourceError::InvalidPayload(_))
        ));
        assert!(matches!(
            parse_scoreboard(br#"{"leagues":[]}"#),
            Err(SourceError::InvalidPayload(_))
        ));
        let empty = parse_scoreboard(br#"{"events":[]}"#).unwrap();
        assert_eq!((empty.games_listed, empty.reports.len()), (0, 0));
    }
}
