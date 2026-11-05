//! Outbound adapter: sportsbook lines from The Odds API (`api.the-odds-api.com`, v4). For a
//! date, one request asks for the NBA games that tip off on that date, Eastern time, with
//! one sportsbook's moneylines (`h2h`), spreads and totals as American odds. The answer
//! lists each game with its start time, the teams by their full names and the sportsbook's
//! outcomes. A game is started once its start time has passed. The service bills credits
//! per market in each request and says in the answer's headers how many are left; the
//! adapter keeps the latest count. One request per date, a 20-second timeout, a 4 MB cap on
//! the body, no redirects. The key travels in the query string, as the service requires;
//! an error's URL is dropped and the key is blanked in an error body before either is
//! logged, and `Debug` output redacts the key.

use std::sync::{Mutex, PoisonError};
use std::time::Duration;

use chrono::{DateTime, NaiveDate, Utc};
use serde::Serialize;
use serde_json::Value;

use crate::application::ports::{
    FuturePrice, FuturesSource, MarketLine, MarketSource, SourceError,
};
use crate::domain::calendar;

use super::feed::{self, read_body, text};

const MAX_BODY_BYTES: usize = 4 * 1024 * 1024;
/// The service's sport key for the league.
const SPORT: &str = "basketball_nba";
/// The service's sport for the title outrights.
const OUTRIGHT_SPORT: &str = "basketball_nba_championship_winner";
/// The markets asked for: the moneyline, the spread and the total; each costs a credit.
const MARKETS: &str = "h2h,spreads,totals";

#[derive(Clone)]
pub struct OddsApiConfig {
    pub api_key: String,
    pub base_url: String,
    /// The sportsbook's key at the service (`fanduel`, `draftkings`, ...).
    pub bookmaker: String,
}

impl std::fmt::Debug for OddsApiConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OddsApiConfig")
            .field("api_key", &"<redacted>")
            .field("base_url", &self.base_url)
            .field("bookmaker", &self.bookmaker)
            .finish()
    }
}

/// What the service said of the key's credits in its latest answer: how many remain in the
/// period, how many were used, and what the last request cost.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Quota {
    pub remaining: Option<u64>,
    pub used: Option<u64>,
    pub last_cost: Option<u64>,
    pub read_at: DateTime<Utc>,
}

#[derive(Debug)]
pub struct OddsApi {
    client: reqwest::Client,
    config: OddsApiConfig,
    quota: Mutex<Option<Quota>>,
}

/// A bookmaker key: letters, digits and underscores, as the service writes them.
pub fn acceptable_bookmaker(key: &str) -> bool {
    !key.is_empty()
        && key.len() <= 40
        && key
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

impl OddsApi {
    pub fn new(config: OddsApiConfig) -> Result<Self, SourceError> {
        if config.api_key.trim().is_empty() {
            return Err(SourceError::NotConfigured(
                "ODDS_API_KEY is empty".to_string(),
            ));
        }
        if !acceptable_bookmaker(&config.bookmaker) {
            return Err(SourceError::NotConfigured(
                "ODDS_API_BOOKMAKER must be the sportsbook's key at the service, in lowercase letters, digits and underscores (for example fanduel)"
                    .to_string(),
            ));
        }
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(20))
            .redirect(reqwest::redirect::Policy::none())
            .user_agent(concat!("nba-elo-simulator/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|e| SourceError::Transport(e.to_string()))?;
        Ok(Self {
            client,
            config: OddsApiConfig {
                api_key: config.api_key.trim().to_string(),
                base_url: config.base_url.trim_end_matches('/').to_string(),
                bookmaker: config.bookmaker,
            },
            quota: Mutex::new(None),
        })
    }

    pub fn base_url(&self) -> &str {
        &self.config.base_url
    }

    pub fn bookmaker(&self) -> &str {
        &self.config.bookmaker
    }

    /// The credits as the latest answer reported them; none before the first answer.
    pub fn quota(&self) -> Option<Quota> {
        *self.quota.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn remember(&self, headers: &reqwest::header::HeaderMap) {
        let count = |name: &str| -> Option<u64> {
            headers
                .get(name)?
                .to_str()
                .ok()?
                .trim()
                .parse::<f64>()
                .ok()
                .filter(|n| n.is_finite() && *n >= 0.0)
                .map(|n| n.round() as u64)
        };
        let quota = Quota {
            remaining: count("x-requests-remaining"),
            used: count("x-requests-used"),
            last_cost: count("x-requests-last"),
            read_at: Utc::now(),
        };
        if quota.remaining.is_some() || quota.used.is_some() {
            *self.quota.lock().unwrap_or_else(PoisonError::into_inner) = Some(quota);
        }
    }

    /// The odds answer for the games of a date, as bytes.
    async fn odds(&self, date: NaiveDate) -> Result<Vec<u8>, SourceError> {
        let (from, to) = calendar::day_span(date);
        self.fetch(
            SPORT,
            &[
                ("markets", MARKETS),
                ("commenceTimeFrom", &stamp(from)),
                ("commenceTimeTo", &stamp(to)),
            ],
        )
        .await
    }

    /// The outright answer on the title, as bytes: one market, one credit.
    async fn outrights(&self) -> Result<Vec<u8>, SourceError> {
        self.fetch(OUTRIGHT_SPORT, &[("markets", "outrights")])
            .await
    }

    /// One odds request of `sport` with `extra` beside the key, the sportsbook and the
    /// formats; the quota remembered, the body capped, the key blanked from an error.
    async fn fetch(&self, sport: &str, extra: &[(&str, &str)]) -> Result<Vec<u8>, SourceError> {
        let url = format!("{}/v4/sports/{sport}/odds", self.config.base_url);
        let mut query: Vec<(&str, &str)> = vec![
            ("apiKey", self.config.api_key.as_str()),
            ("bookmakers", self.config.bookmaker.as_str()),
            ("oddsFormat", "american"),
            ("dateFormat", "iso"),
        ];
        query.extend_from_slice(extra);
        let resp = self
            .client
            .get(&url)
            .query(&query)
            .send()
            .await
            .map_err(transport)?;
        self.remember(resp.headers());
        let (status, bytes) = read_body(resp, MAX_BODY_BYTES).await?;
        if !status.is_success() {
            // The service does not echo the key; a proxy in its place might.
            let shown = String::from_utf8_lossy(&bytes).replace(&self.config.api_key, "<key>");
            return Err(feed::rejected(status, shown.as_bytes()));
        }
        Ok(bytes)
    }
}

/// A transport error without its URL, which carries the key.
fn transport(e: reqwest::Error) -> SourceError {
    SourceError::Transport(e.without_url().to_string())
}

/// A time as the service wants it: ISO 8601 in UTC to the second.
fn stamp(t: DateTime<Utc>) -> String {
    t.format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

fn finite(v: Option<&Value>) -> Option<f64> {
    v?.as_f64().filter(|n| n.is_finite())
}

/// The outcome of a market named for a side.
fn outcome<'a>(market: Option<&'a Value>, name: &str) -> Option<&'a Value> {
    market?
        .get("outcomes")?
        .as_array()?
        .iter()
        .find(|o| text(o, "name").is_some_and(|n| n.eq_ignore_ascii_case(name)))
}

/// An outcome's price, when usable.
fn price(market: Option<&Value>, side: &str) -> Option<f64> {
    finite(outcome(market, side).and_then(|o| o.get("price"))).filter(|o| feed::usable_odds(*o))
}

/// An outcome's point: a spread or a total.
fn point(market: Option<&Value>, side: &str) -> Option<f64> {
    finite(outcome(market, side).and_then(|o| o.get("point")))
}

/// The lines in one odds answer, read at `now`: for each game, the first sportsbook's
/// moneyline, spread and total. A game without a moneyline or a spread is skipped, as is
/// one whose two spreads do not mirror each other (the two sides would not be one line).
/// An answer that lists games not yet started but no sportsbook for any game is an error,
/// not an empty night: the bookmaker key is unknown to the service or does not price the
/// league. A started game without the sportsbook is normal: the book pulls its lines once
/// the game is under way.
pub fn parse_lines(bytes: &[u8], now: DateTime<Utc>) -> Result<Vec<MarketLine>, SourceError> {
    let doc: Value = serde_json::from_slice(bytes)
        .map_err(|e| SourceError::InvalidPayload(format!("not an odds answer: {e}")))?;
    let events = doc
        .as_array()
        .ok_or_else(|| SourceError::InvalidPayload("not a list of games".to_string()))?;
    let mut out = Vec::new();
    let (mut with_book, mut unstarted_without_book) = (0usize, 0usize);
    for event in events {
        let (Some(home_name), Some(away_name)) =
            (text(event, "home_team"), text(event, "away_team"))
        else {
            continue;
        };
        let start_time = text(event, "commence_time").and_then(|d| {
            DateTime::parse_from_rfc3339(&d)
                .map(|t| t.with_timezone(&Utc))
                .ok()
        });
        let Some(book) = event
            .get("bookmakers")
            .and_then(Value::as_array)
            .and_then(|b| b.first())
        else {
            if start_time.is_none_or(|s| s > now) {
                unstarted_without_book += 1;
            }
            continue;
        };
        with_book += 1;
        let provider: String = text(book, "title")
            .or_else(|| text(book, "key"))
            .unwrap_or_else(|| "sportsbook".to_string())
            .chars()
            .take(40)
            .collect();
        let markets = book.get("markets").and_then(Value::as_array);
        let market = |key: &str| -> Option<&Value> {
            markets?
                .iter()
                .find(|m| text(m, "key").is_some_and(|k| k == key))
        };
        let h2h = market("h2h");
        let spreads = market("spreads");
        let totals = market("totals");
        let home_spread = point(spreads, &home_name).filter(|x| feed::usable_spread(*x));
        let away_spread = point(spreads, &away_name).filter(|x| feed::usable_spread(*x));
        // The two sides carry the one line with opposite signs; anything else is not a line.
        let spread_ok = match (home_spread, away_spread) {
            (Some(h), Some(a)) => (h + a).abs() < 1e-9,
            (None, None) => true,
            _ => false,
        };
        let line = MarketLine {
            home_spread: home_spread.filter(|_| spread_ok),
            home_spread_odds: price(spreads, &home_name).filter(|_| spread_ok),
            away_spread_odds: price(spreads, &away_name).filter(|_| spread_ok),
            home_moneyline: price(h2h, &home_name),
            away_moneyline: price(h2h, &away_name),
            total: point(totals, "Over").filter(|t| feed::usable_total(*t)),
            start_time,
            started: start_time.is_some_and(|s| s <= now),
            home_abbr: String::new(),
            away_abbr: String::new(),
            home_name,
            away_name,
            provider,
        };
        if line.home_spread.is_some() || line.home_moneyline.is_some() {
            out.push(line);
        }
    }
    if with_book == 0 && unstarted_without_book > 0 {
        return Err(SourceError::InvalidPayload(format!(
            "none of the {} games lists the sportsbook asked for; check ODDS_API_BOOKMAKER",
            events.len()
        )));
    }
    Ok(out)
}

impl MarketSource for OddsApi {
    fn name(&self) -> &'static str {
        "the-odds-api"
    }

    async fn lines_on(&self, date: NaiveDate) -> Result<Vec<MarketLine>, SourceError> {
        parse_lines(&self.odds(date).await?, Utc::now())
    }
}

/// The sportsbook's name and its outright price on each team in an outright answer: the
/// one event's first sportsbook's `outrights` market. An answer with no event, no
/// sportsbook or fewer than two usable prices is an error: the sportsbook does not price
/// the title, or the service does not know it.
pub fn parse_outrights(bytes: &[u8]) -> Result<(String, Vec<FuturePrice>), SourceError> {
    let doc: Value = serde_json::from_slice(bytes)
        .map_err(|e| SourceError::InvalidPayload(format!("not an outright answer: {e}")))?;
    let events = doc
        .as_array()
        .ok_or_else(|| SourceError::InvalidPayload("not a list of events".to_string()))?;
    let book = events
        .iter()
        .filter_map(|e| e.get("bookmakers")?.as_array()?.first())
        .next()
        .ok_or_else(|| {
            SourceError::InvalidPayload(
                "no sportsbook prices the title; check ODDS_API_BOOKMAKER".to_string(),
            )
        })?;
    let provider: String = text(book, "title")
        .or_else(|| text(book, "key"))
        .unwrap_or_default()
        .chars()
        .take(40)
        .collect();
    let market = book
        .get("markets")
        .and_then(Value::as_array)
        .and_then(|ms| {
            ms.iter()
                .find(|m| text(m, "key").as_deref() == Some("outrights"))
        })
        .ok_or_else(|| SourceError::InvalidPayload("no outright market".to_string()))?;
    let mut prices: Vec<FuturePrice> = market
        .get("outcomes")
        .and_then(Value::as_array)
        .map(|os| {
            os.iter()
                .filter_map(|o| {
                    let name = text(o, "name").filter(|n| !n.is_empty())?;
                    let price = finite(o.get("price")).filter(|p| feed::usable_odds(*p))?;
                    Some(FuturePrice { name, price })
                })
                .collect()
        })
        .unwrap_or_default();
    // One price per name: the first listed.
    let mut seen = std::collections::HashSet::new();
    prices.retain(|p| seen.insert(p.name.to_ascii_lowercase()));
    if prices.len() < 2 {
        return Err(SourceError::InvalidPayload(format!(
            "{} usable outright price(s); a field needs two",
            prices.len()
        )));
    }
    Ok((provider, prices))
}

impl FuturesSource for OddsApi {
    fn name(&self) -> &'static str {
        "the-odds-api"
    }

    async fn title_futures(&self) -> Result<(String, Vec<FuturePrice>), SourceError> {
        parse_outrights(&self.outrights().await?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Two games as the service lists them: one priced, one started and priced, one with
    /// no sportsbook.
    const ODDS: &str = r#"[
      {"id":"a1","sport_key":"basketball_nba","commence_time":"2026-11-03T00:10:00Z",
       "home_team":"Golden State Warriors","away_team":"Los Angeles Clippers",
       "bookmakers":[{"key":"fanduel","title":"FanDuel","last_update":"2026-11-02T20:00:00Z","markets":[
         {"key":"h2h","outcomes":[{"name":"Golden State Warriors","price":-136},{"name":"Los Angeles Clippers","price":115}]},
         {"key":"spreads","outcomes":[{"name":"Golden State Warriors","price":-108,"point":-2.5},{"name":"Los Angeles Clippers","price":-112,"point":2.5}]},
         {"key":"totals","outcomes":[{"name":"Over","price":-110,"point":219.5},{"name":"Under","price":-110,"point":219.5}]}]}]},
      {"id":"a2","sport_key":"basketball_nba","commence_time":"2026-11-02T23:40:00Z",
       "home_team":"Boston Celtics","away_team":"Orlando Magic",
       "bookmakers":[{"key":"fanduel","title":"FanDuel","markets":[
         {"key":"h2h","outcomes":[{"name":"Boston Celtics","price":-250},{"name":"Orlando Magic","price":205}]},
         {"key":"spreads","outcomes":[{"name":"Boston Celtics","price":-110,"point":-6.5},{"name":"Orlando Magic","price":-110,"point":4.5}]}]}]},
      {"id":"a3","sport_key":"basketball_nba","commence_time":"2026-11-03T02:00:00Z",
       "home_team":"Los Angeles Lakers","away_team":"Minnesota Timberwolves","bookmakers":[]}
    ]"#;

    #[test]
    fn the_lines_are_read_with_their_start_and_sportsbook() {
        let now = DateTime::parse_from_rfc3339("2026-11-03T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let lines = parse_lines(ODDS.as_bytes(), now).unwrap();
        assert_eq!(lines.len(), 2);
        let gsw = &lines[0];
        assert_eq!(
            (
                gsw.home_name.as_str(),
                gsw.away_name.as_str(),
                gsw.provider.as_str()
            ),
            ("Golden State Warriors", "Los Angeles Clippers", "FanDuel")
        );
        assert_eq!(
            (
                gsw.home_spread,
                gsw.home_spread_odds,
                gsw.away_spread_odds,
                gsw.home_moneyline,
                gsw.away_moneyline,
                gsw.total
            ),
            (
                Some(-2.5),
                Some(-108.0),
                Some(-112.0),
                Some(-136.0),
                Some(115.0),
                Some(219.5)
            )
        );
        assert!(!gsw.started && gsw.start_time.is_some());
        // The Celtics tipped off twenty minutes before the read, and the two spreads do not
        // mirror each other, so the spread is dropped and the moneyline kept.
        let bos = &lines[1];
        assert!(bos.started);
        assert_eq!(
            (
                bos.home_spread,
                bos.home_spread_odds,
                bos.home_moneyline,
                bos.total
            ),
            (None, None, Some(-250.0), None)
        );
    }

    #[test]
    fn an_answer_that_is_not_a_list_is_rejected_and_odd_values_are_dropped() {
        assert!(parse_lines(br#"{"message":"Invalid api key"}"#, Utc::now()).is_err());
        let odd = r#"[{"home_team":"A","away_team":"B","commence_time":"bad","bookmakers":[{"key":"x","markets":[
          {"key":"h2h","outcomes":[{"name":"A","price":-50},{"name":"B","price":1e9}]},
          {"key":"spreads","outcomes":[{"name":"A","price":-110,"point":-70},{"name":"B","price":-110,"point":70}]},
          {"key":"totals","outcomes":[{"name":"Over","price":-110,"point":50}]}]}]}]"#;
        assert!(parse_lines(odd.as_bytes(), Utc::now()).unwrap().is_empty());
    }

    #[test]
    fn games_without_the_sportsbook_are_an_error_and_an_empty_night_is_not() {
        let unknown = r#"[{"home_team":"A","away_team":"B","commence_time":"2026-11-03T00:00:00Z","bookmakers":[]}]"#;
        let before = DateTime::parse_from_rfc3339("2026-11-02T23:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let e = parse_lines(unknown.as_bytes(), before).unwrap_err();
        assert!(e.to_string().contains("ODDS_API_BOOKMAKER"), "{e}");
        // The same game once under way: the book pulled its lines, which is no error.
        let after = before + chrono::Duration::hours(2);
        assert!(parse_lines(unknown.as_bytes(), after).unwrap().is_empty());
        assert!(parse_lines(b"[]", Utc::now()).unwrap().is_empty());
        let (from, to) = calendar::day_span(NaiveDate::from_ymd_opt(2026, 11, 2).unwrap());
        assert_eq!(
            (stamp(from).as_str(), stamp(to).as_str()),
            ("2026-11-02T05:00:00Z", "2026-11-03T04:59:59Z")
        );
    }

    const OUTRIGHTS: &str = r#"[
      {"id":"o1","sport_key":"basketball_nba_championship_winner","commence_time":"2027-06-30T00:00:00Z",
       "home_team":null,"away_team":null,
       "bookmakers":[{"key":"fanduel","title":"FanDuel","last_update":"2026-11-05T12:00:00Z","markets":[
         {"key":"outrights","last_update":"2026-11-05T12:00:00Z","outcomes":[
           {"name":"Oklahoma City Thunder","price":300},
           {"name":"Boston Celtics","price":450},
           {"name":"Denver Nuggets","price":900},
           {"name":"Denver Nuggets","price":1000},
           {"name":"Washington Wizards","price":50000},
           {"name":"Nobody","price":50},
           {"name":"","price":400}]}]}]}
    ]"#;

    #[test]
    fn the_outright_prices_are_read_once_per_team_with_the_sportsbook() {
        let (provider, prices) = parse_outrights(OUTRIGHTS.as_bytes()).unwrap();
        assert_eq!(provider, "FanDuel");
        let names: Vec<&str> = prices.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(
            names,
            vec![
                "Oklahoma City Thunder",
                "Boston Celtics",
                "Denver Nuggets",
                "Washington Wizards"
            ],
            "a repeated name keeps its first price; +50 and a blank name are dropped"
        );
        assert_eq!(prices[2].price, 900.0);
        assert!(matches!(
            parse_outrights(b"[]"),
            Err(SourceError::InvalidPayload(m)) if m.contains("ODDS_API_BOOKMAKER")
        ));
        assert!(matches!(
            parse_outrights(br#"[{"bookmakers":[{"key":"fanduel","markets":[{"key":"h2h","outcomes":[]}]}]}]"#),
            Err(SourceError::InvalidPayload(m)) if m.contains("no outright market")
        ));
        assert!(matches!(
            parse_outrights(br#"[{"bookmakers":[{"key":"fanduel","markets":[{"key":"outrights","outcomes":[{"name":"A","price":300}]}]}]}]"#),
            Err(SourceError::InvalidPayload(m)) if m.contains("needs two")
        ));
        assert!(matches!(
            parse_outrights(b"<html>"),
            Err(SourceError::InvalidPayload(_))
        ));
    }

    #[test]
    fn the_configuration_is_checked_and_redacted() {
        assert!(acceptable_bookmaker("fanduel") && acceptable_bookmaker("draftkings"));
        assert!(
            !acceptable_bookmaker("")
                && !acceptable_bookmaker("Fan Duel")
                && !acceptable_bookmaker("a=b")
        );
        let cfg = OddsApiConfig {
            api_key: "secret".into(),
            base_url: "https://api.the-odds-api.com/".into(),
            bookmaker: "fanduel".into(),
        };
        assert!(!format!("{cfg:?}").contains("secret"));
        let api = OddsApi::new(cfg).unwrap();
        assert_eq!(api.base_url(), "https://api.the-odds-api.com");
        assert!(api.quota().is_none());
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert("x-requests-remaining", "18420".parse().unwrap());
        headers.insert("x-requests-used", "1580".parse().unwrap());
        headers.insert("x-requests-last", "3".parse().unwrap());
        api.remember(&headers);
        let q = api.quota().unwrap();
        assert_eq!(
            (q.remaining, q.used, q.last_cost),
            (Some(18420), Some(1580), Some(3))
        );
        assert!(OddsApi::new(OddsApiConfig {
            api_key: " ".into(),
            base_url: String::new(),
            bookmaker: "fanduel".into()
        })
        .is_err());
    }
}
