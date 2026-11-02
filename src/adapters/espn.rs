//! Outbound adapter: game reports and sportsbook lines from ESPN's public scoreboard feed.
//! For a date, the feed lists each game with the teams, a link to the game report when ESPN
//! published one (the Associated Press recap), and, for games not yet played, one
//! sportsbook's lines (DraftKings at the time of writing): the spread and both moneylines
//! at the open and now. One request per date, a 20-second timeout, a 4 MB cap on the
//! body, no redirects. Only `https` links on an ESPN host are kept.

use std::time::Duration;

use chrono::NaiveDate;
use serde_json::Value;

use crate::application::ports::{
    MarketLine, MarketSource, ReportCandidate, ReportListing, ReportSource, SourceError,
};

const MAX_BODY_BYTES: usize = 4 * 1024 * 1024;
const MAX_TITLE_CHARS: usize = 200;

#[derive(Debug)]
pub struct EspnScoreboard {
    client: reqwest::Client,
    base_url: String,
}

impl EspnScoreboard {
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

/// The home and away teams of a competition, each as (abbreviation, display name).
fn sides(competition: &Value) -> Option<((String, String), (String, String))> {
    let competitors = competition.get("competitors").and_then(Value::as_array)?;
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
    Some((side("home")?, side("away")?))
}

/// A number the feed gives as text or as a number ("-2.5", "+114", 219.5), with the words
/// it uses for a round value: `even` for one.
fn number(v: Option<&Value>, even: f64) -> Option<f64> {
    let n = match v? {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => {
            let t = s.trim();
            if ["even", "ev", "pk", "pick"]
                .iter()
                .any(|w| t.eq_ignore_ascii_case(w))
            {
                Some(even)
            } else {
                t.trim_start_matches('+').parse::<f64>().ok()
            }
        }
        _ => None,
    }?;
    n.is_finite().then_some(n)
}

/// A moneyline or a bet's price: EVEN is +100; a value between -100 and +100, or beyond
/// 100,000 either way, is none.
fn odds_number(v: Option<&Value>) -> Option<f64> {
    number(v, 100.0).filter(|o| (100.0..=100_000.0).contains(&o.abs()))
}

/// A spread: PK or EVEN is 0, and one wider than any game's (60 points) is none.
fn spread_number(v: Option<&Value>) -> Option<f64> {
    number(v, 0.0).filter(|s| s.abs() <= 60.0)
}

/// Both sides of a market at one price: `close` (the current line before the game) when
/// both sides have a usable value there, else `open` when both do, so the two sides never
/// mix prices. A suspended close ("OFF") falls back to the open; none when neither works.
fn latest_pair(
    market: Option<&Value>,
    usable: impl Fn(&Value) -> bool,
) -> Option<(&Value, &Value)> {
    let market = market?;
    let (home, away) = (market.get("home")?, market.get("away")?);
    ["close", "open"].iter().find_map(|key| {
        let pair = home.get(*key).zip(away.get(*key))?;
        (usable(pair.0) && usable(pair.1)).then_some(pair)
    })
}

/// The sportsbook lines in one scoreboard answer: for each game with odds, the first
/// provider's spread, moneylines and total. Older answers carry the moneylines and the
/// spread directly on the odds object; those are read when the newer markets are missing.
pub fn parse_lines(bytes: &[u8]) -> Result<Vec<MarketLine>, SourceError> {
    let doc: Value = serde_json::from_slice(bytes)
        .map_err(|e| SourceError::InvalidPayload(format!("not a scoreboard: {e}")))?;
    let events = doc
        .get("events")
        .and_then(Value::as_array)
        .ok_or_else(|| SourceError::InvalidPayload("no events".to_string()))?;
    let mut out = Vec::new();
    for event in events {
        let Some(competition) = event
            .get("competitions")
            .and_then(Value::as_array)
            .and_then(|c| c.first())
        else {
            continue;
        };
        let Some(((home_abbr, home_name), (away_abbr, away_name))) = sides(competition) else {
            continue;
        };
        let Some(odds) = competition
            .get("odds")
            .and_then(Value::as_array)
            .and_then(|o| o.first())
        else {
            continue;
        };
        let provider = odds
            .get("provider")
            .and_then(|p| text(p, "displayName").or_else(|| text(p, "name")))
            .unwrap_or_else(|| "sportsbook".to_string())
            .chars()
            .take(40)
            .collect();
        let spread = latest_pair(odds.get("pointSpread"), |s| {
            spread_number(s.get("line")).is_some()
        });
        let ml = latest_pair(odds.get("moneyline"), |s| {
            odds_number(s.get("odds")).is_some()
        });
        let side_odds = |side: &str, field: &str| -> Option<f64> {
            odds.get(side).and_then(|s| odds_number(s.get(field)))
        };
        // The start and the state: `pre` before the game, `in` and `post` after it starts.
        let start_time = text(event, "date").and_then(|d| {
            chrono::DateTime::parse_from_rfc3339(&d)
                .map(|t| t.with_timezone(&chrono::Utc))
                .ok()
                .or_else(|| {
                    chrono::NaiveDateTime::parse_from_str(&d, "%Y-%m-%dT%H:%MZ")
                        .ok()
                        .map(|t| t.and_utc())
                })
        });
        let started = event
            .get("status")
            .and_then(|s| s.get("type"))
            .and_then(|t| text(t, "state"))
            .is_some_and(|s| s != "pre");
        let line = MarketLine {
            start_time,
            started,
            home_abbr,
            away_abbr,
            home_name,
            away_name,
            provider,
            // The newer markets first; the older shape's fields only when a market is missing.
            home_spread: match spread {
                Some((h, _)) => spread_number(h.get("line")),
                None => spread_number(odds.get("spread")),
            },
            home_spread_odds: match spread {
                Some((h, _)) => odds_number(h.get("odds")),
                None => side_odds("homeTeamOdds", "spreadOdds"),
            },
            away_spread_odds: match spread {
                Some((_, a)) => odds_number(a.get("odds")),
                None => side_odds("awayTeamOdds", "spreadOdds"),
            },
            home_moneyline: match ml {
                Some((h, _)) => odds_number(h.get("odds")),
                None => side_odds("homeTeamOdds", "moneyLine"),
            },
            away_moneyline: match ml {
                Some((_, a)) => odds_number(a.get("odds")),
                None => side_odds("awayTeamOdds", "moneyLine"),
            },
            // An NBA total lies between 100 and 400 points.
            total: number(odds.get("overUnder"), 0.0).filter(|t| (100.0..=400.0).contains(t)),
        };
        if line.home_spread.is_some() || line.home_moneyline.is_some() {
            out.push(line);
        }
    }
    Ok(out)
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
        let Some(((home_abbr, home_name), (away_abbr, away_name))) = sides(competition) else {
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

impl ReportSource for EspnScoreboard {
    fn name(&self) -> &'static str {
        "espn"
    }

    async fn reports_on(&self, date: NaiveDate) -> Result<ReportListing, SourceError> {
        parse_scoreboard(&self.scoreboard(date).await?)
    }
}

impl MarketSource for EspnScoreboard {
    async fn lines_on(&self, date: NaiveDate) -> Result<Vec<MarketLine>, SourceError> {
        parse_lines(&self.scoreboard(date).await?)
    }
}

impl EspnScoreboard {
    /// The scoreboard answer for a date, as bytes.
    async fn scoreboard(&self, date: NaiveDate) -> Result<Vec<u8>, SourceError> {
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
        Ok(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Two games with lines in the two shapes the feed has used, and one without.
    const LINES: &str = r#"{"events":[
      {"id":"401909905","date":"2026-11-02T01:30Z","status":{"type":{"state":"pre"}},"competitions":[{"competitors":[
         {"homeAway":"home","team":{"abbreviation":"GS","displayName":"Golden State Warriors"}},
         {"homeAway":"away","team":{"abbreviation":"LAC","displayName":"LA Clippers"}}],
       "odds":[{"provider":{"name":"DraftKings","displayName":"DraftKings"},"details":"GS -2.5",
         "overUnder":219.5,"spread":-2.5,
         "moneyline":{"home":{"close":{"odds":"-135"},"open":{"odds":"-130"}},
                      "away":{"close":{"odds":"+114"},"open":{"odds":"+110"}}},
         "pointSpread":{"home":{"close":{"line":"-2.5","odds":"-105"},"open":{"line":"-2.5","odds":"-105"}},
                        "away":{"close":{"line":"+2.5","odds":"-115"}}}}]}]},
      {"id":"401360460","date":"2022-01-15T20:00:00Z","status":{"type":{"state":"post"}},"competitions":[{"competitors":[
         {"homeAway":"home","team":{"abbreviation":"MIL","displayName":"Milwaukee Bucks"}},
         {"homeAway":"away","team":{"abbreviation":"NY","displayName":"New York Knicks"}}],
       "odds":[{"provider":{"name":"consensus"},"spread":-7.5,"overUnder":221,
         "homeTeamOdds":{"moneyLine":-316},"awayTeamOdds":{"moneyLine":"EVEN"}}]}]},
      {"id":"3","competitions":[{"competitors":[
         {"homeAway":"home","team":{"abbreviation":"BOS","displayName":"Boston Celtics"}},
         {"homeAway":"away","team":{"abbreviation":"ORL","displayName":"Orlando Magic"}}]}]}
    ]}"#;

    #[test]
    fn the_lines_are_read_at_their_latest_price_in_either_shape() {
        let lines = parse_lines(LINES.as_bytes()).unwrap();
        assert_eq!(lines.len(), 2);
        let gs = &lines[0];
        assert_eq!(
            (
                gs.home_abbr.as_str(),
                gs.away_abbr.as_str(),
                gs.provider.as_str()
            ),
            ("GS", "LAC", "DraftKings")
        );
        assert_eq!(
            (gs.home_spread, gs.home_spread_odds, gs.away_spread_odds),
            (Some(-2.5), Some(-105.0), Some(-115.0))
        );
        assert_eq!(
            (gs.home_moneyline, gs.away_moneyline),
            (Some(-135.0), Some(114.0))
        );
        assert_eq!(gs.total, Some(219.5));
        assert_eq!(
            (gs.start_time.map(|t| t.to_rfc3339()), gs.started),
            (Some("2026-11-02T01:30:00+00:00".to_string()), false)
        );
        assert!(lines[1].started && lines[1].start_time.is_some());
        let mil = &lines[1];
        assert_eq!((mil.home_spread, mil.home_spread_odds), (Some(-7.5), None));
        assert_eq!(
            (mil.home_moneyline, mil.away_moneyline),
            (Some(-316.0), Some(100.0))
        );
        assert_eq!(mil.provider, "consensus");
        assert!(parse_lines(b"{}").is_err());
    }

    #[test]
    fn the_two_sides_share_one_price_and_odd_values_are_dropped() {
        let doc = r#"{"events":[
          {"competitions":[{"competitors":[
             {"homeAway":"home","team":{"abbreviation":"NY","displayName":"New York Knicks"}},
             {"homeAway":"away","team":{"abbreviation":"BOS","displayName":"Boston Celtics"}}],
           "odds":[{"provider":{"name":"DraftKings"},"overUnder":"OFF",
             "moneyline":{"home":{"close":{"odds":"-150"},"open":{"odds":"-140"}},
                          "away":{"open":{"odds":"+120"}}},
             "pointSpread":{"home":{"close":{"line":"PK","odds":"-110"}},
                            "away":{"close":{"line":"PK","odds":"+50"}}}}]}]},
          {"competitions":[{"competitors":[
             {"homeAway":"home","team":{"abbreviation":"LAL","displayName":"Los Angeles Lakers"}}],
           "odds":[{"spread":-3}]}]},
          {"competitions":[{"competitors":[
             {"homeAway":"home","team":{"abbreviation":"MIA","displayName":"Miami Heat"}},
             {"homeAway":"away","team":{"abbreviation":"ORL","displayName":"Orlando Magic"}}],
           "odds":[{"spread":"300","homeTeamOdds":{"moneyLine":-150,"spreadOdds":-112},
                    "awayTeamOdds":{"moneyLine":"+130","spreadOdds":"-108"}}]}]},
          {"competitions":[{"competitors":[
             {"homeAway":"home","team":{"abbreviation":"DEN","displayName":"Denver Nuggets"}},
             {"homeAway":"away","team":{"abbreviation":"UTAH","displayName":"Utah Jazz"}}],
           "odds":[{"spread":-4,"overUnder":2195,
             "moneyline":{"home":{"close":{"odds":"OFF"},"open":{"odds":"-170"}},
                          "away":{"close":{"odds":"OFF"},"open":{"odds":"+145"}}},
             "pointSpread":{"home":{"close":{"line":"OFF"}},"away":{"close":{"line":"OFF"}}}}]}]}
        ]}"#;
        let lines = parse_lines(doc.as_bytes()).unwrap();
        // The game without both teams is skipped.
        assert_eq!(lines.len(), 3);
        let ny = &lines[0];
        // No date and no status: no start time, and not taken as started.
        assert_eq!((ny.start_time, ny.started), (None, false));
        // The away moneyline has no closing price, so both sides read the opening one.
        assert_eq!(
            (ny.home_moneyline, ny.away_moneyline),
            (Some(-140.0), Some(120.0))
        );
        // A pick'em spread is 0, a price between -100 and +100 and an OFF total are none.
        assert_eq!(
            (
                ny.home_spread,
                ny.home_spread_odds,
                ny.away_spread_odds,
                ny.total
            ),
            (Some(0.0), Some(-110.0), None, None)
        );
        let mia = &lines[1];
        // The older shape's spread prices are read; a 300-point spread is none.
        assert_eq!(
            (mia.home_spread, mia.home_spread_odds, mia.away_spread_odds),
            (None, Some(-112.0), Some(-108.0))
        );
        assert_eq!(
            (mia.home_moneyline, mia.away_moneyline),
            (Some(-150.0), Some(130.0))
        );
        let den = &lines[2];
        // A suspended close falls back to the open; a spread market with no usable price
        // falls back to the older field; a 2,195-point total is none.
        assert_eq!(
            (den.home_moneyline, den.away_moneyline),
            (Some(-170.0), Some(145.0))
        );
        assert_eq!((den.home_spread, den.total), (Some(-4.0), None));
    }

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
