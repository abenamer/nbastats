//! CSV parsers for the two known Elo dataset layouts.
//!
//! * `fivethirtyeight_nba_elo`: `date,season,neutral,playoff,team1,team2,elo1_pre,elo2_pre,
//!   elo_prob1,elo_prob2,elo1_post,elo2_post,...,score1,score2[,is_home]` – FiveThirtyEight's
//!   `nba_elo.csv` (one row per game, team1 = home) and Neil Paine's archive (two rows per
//!   game, one per team perspective, with `is_home`).
//! * `fivethirtyeight_nbaallelo`: `gameorder,game_id,lg_id,_iscopy,year_id,date_game,...` –
//!   the older `nbaallelo.csv` with one row per team per game.

use std::collections::HashMap;
use std::io::Read;

use chrono::NaiveDate;

#[derive(Debug, Clone, PartialEq)]
pub struct ParsedGame {
    pub season: i32,
    pub date: NaiveDate,
    pub seq: i16,
    pub home: String,
    pub away: String,
    pub neutral: bool,
    pub playoff: bool,
    pub home_score: Option<i32>,
    pub away_score: Option<i32>,
    pub home_elo_pre: f64,
    pub away_elo_pre: f64,
    pub home_elo_post: f64,
    pub away_elo_post: f64,
    pub home_win_prob: Option<f64>,
}

#[derive(Debug, Default)]
pub struct ParseReport {
    pub format: String,
    pub rows_read: i64,
    pub rows_rejected: i64,
    pub rejections: Vec<String>,
    pub games: Vec<ParsedGame>,
}

#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    #[error("unrecognised CSV layout: header {0:?} is neither nba_elo.csv nor nbaallelo.csv")]
    UnknownFormat(Vec<String>),
    #[error("CSV error: {0}")]
    Csv(#[from] csv::Error),
    #[error("file is empty")]
    Empty,
}

pub fn detect_format(header: &[String]) -> Option<&'static str> {
    let has = |c: &str| header.iter().any(|h| h == c);
    if has("team1") && has("team2") && has("elo1_pre") && has("elo2_pre") && has("season") && has("date") {
        Some("fivethirtyeight_nba_elo")
    } else if has("team_id") && has("opp_id") && has("elo_i") && has("elo_n") && has("year_id") && has("game_location") {
        Some("fivethirtyeight_nbaallelo")
    } else {
        None
    }
}

fn parse_f64(s: &str) -> Option<f64> {
    let s = s.trim();
    if s.is_empty() || s.eq_ignore_ascii_case("NA") || s.eq_ignore_ascii_case("NaN") {
        return None;
    }
    s.parse().ok()
}

fn parse_i32(s: &str) -> Option<i32> {
    parse_f64(s).map(|f| f.round() as i32)
}

fn parse_bool(s: &str) -> bool {
    matches!(s.trim().to_ascii_lowercase().as_str(), "1" | "t" | "true" | "yes" | "y")
}

fn parse_date(s: &str) -> Option<NaiveDate> {
    let s = s.trim();
    NaiveDate::parse_from_str(s, "%Y-%m-%d")
        .or_else(|_| NaiveDate::parse_from_str(s, "%m/%d/%Y"))
        .or_else(|_| NaiveDate::parse_from_str(s, "%Y/%m/%d"))
        .ok()
}

pub fn parse<R: Read>(reader: R) -> Result<ParseReport, ParseError> {
    let mut rdr = csv::ReaderBuilder::new().flexible(true).trim(csv::Trim::All).from_reader(reader);
    let header: Vec<String> = rdr.headers()?.iter().map(|h| h.trim().to_string()).collect();
    if header.is_empty() {
        return Err(ParseError::Empty);
    }
    let format = detect_format(&header).ok_or_else(|| ParseError::UnknownFormat(header.clone()))?;
    let col: HashMap<&str, usize> = header.iter().enumerate().map(|(i, h)| (h.as_str(), i)).collect();
    let mut report = ParseReport { format: format.to_string(), ..Default::default() };
    // occurrences per (date, unordered pair) to derive `seq` and to deduplicate perspective rows
    let mut seen: HashMap<(NaiveDate, String, String), (u32, u32)> = HashMap::new();
    let get = |rec: &csv::StringRecord, name: &str| -> String { col.get(name).and_then(|&i| rec.get(i)).unwrap_or("").to_string() };
    for (line_no, rec) in rdr.records().enumerate() {
        let rec = match rec {
            Ok(r) => r,
            Err(e) => {
                report.rows_rejected += 1;
                if report.rejections.len() < 20 {
                    report.rejections.push(format!("row {}: {e}", line_no + 2));
                }
                continue;
            }
        };
        report.rows_read += 1;
        let parsed: Result<Option<ParsedGame>, String> = match format {
            "fivethirtyeight_nba_elo" => parse_nba_elo_row(&rec, &get, &col, &mut seen),
            _ => parse_nbaallelo_row(&rec, &get, &mut seen),
        };
        match parsed {
            Ok(Some(g)) => report.games.push(g),
            Ok(None) => {}
            Err(e) => {
                report.rows_rejected += 1;
                if report.rejections.len() < 20 {
                    report.rejections.push(format!("row {}: {e}", line_no + 2));
                }
            }
        }
    }
    Ok(report)
}

type Getter<'a> = dyn Fn(&csv::StringRecord, &str) -> String + 'a;

fn parse_nba_elo_row(
    rec: &csv::StringRecord,
    get: &Getter,
    col: &HashMap<&str, usize>,
    seen: &mut HashMap<(NaiveDate, String, String), (u32, u32)>,
) -> Result<Option<ParsedGame>, String> {
    let date = parse_date(&get(rec, "date")).ok_or("bad date")?;
    let season: i32 = get(rec, "season").parse().map_err(|_| "bad season")?;
    let neutral = parse_bool(&get(rec, "neutral"));
    let playoff = parse_bool(&get(rec, "playoff"));
    let team1 = get(rec, "team1");
    let team2 = get(rec, "team2");
    if team1.is_empty() || team2.is_empty() || team1 == team2 {
        return Err("bad team codes".into());
    }
    let elo1_pre = parse_f64(&get(rec, "elo1_pre")).ok_or("bad elo1_pre")?;
    let elo2_pre = parse_f64(&get(rec, "elo2_pre")).ok_or("bad elo2_pre")?;
    let score1 = parse_i32(&get(rec, "score1"));
    let score2 = parse_i32(&get(rec, "score2"));
    let elo1_post = parse_f64(&get(rec, "elo1_post")).unwrap_or(elo1_pre);
    let elo2_post = parse_f64(&get(rec, "elo2_post")).unwrap_or(elo2_pre);
    let prob1 = parse_f64(&get(rec, "elo_prob1"));
    let has_is_home = col.contains_key("is_home");
    // In the two-perspective layout, keep only the home perspective; neutral games have two
    // identical rows with is_home = 0, of which we keep every other occurrence.
    let team1_is_home = if has_is_home { parse_bool(&get(rec, "is_home")) } else { true };
    let key = (date, team1.clone().min(team2.clone()), team1.clone().max(team2.clone()));
    let (occurrence, kept) = seen.entry(key).or_insert((0, 0));
    let keep = if has_is_home {
        if neutral {
            *occurrence % 2 == 0
        } else {
            team1_is_home
        }
    } else {
        true
    };
    *occurrence += 1;
    if !keep {
        return Ok(None);
    }
    // `seq` counts games already kept for this pair on this date (double-headers).
    let seq = *kept;
    *kept += 1;
    if !(1000.0..=2500.0).contains(&elo1_pre) || !(1000.0..=2500.0).contains(&elo2_pre) {
        return Err("Elo rating out of plausible range".into());
    }
    Ok(Some(ParsedGame {
        season,
        date,
        seq: seq as i16,
        home: team1,
        away: team2,
        neutral,
        playoff,
        home_score: score1,
        away_score: score2,
        home_elo_pre: elo1_pre,
        away_elo_pre: elo2_pre,
        home_elo_post: elo1_post,
        away_elo_post: elo2_post,
        home_win_prob: prob1,
    }))
}

fn parse_nbaallelo_row(
    rec: &csv::StringRecord,
    get: &Getter,
    seen: &mut HashMap<(NaiveDate, String, String), (u32, u32)>,
) -> Result<Option<ParsedGame>, String> {
    if parse_bool(&get(rec, "_iscopy")) {
        return Ok(None);
    }
    let date = parse_date(&get(rec, "date_game")).ok_or("bad date_game")?;
    let season: i32 = get(rec, "year_id").parse().map_err(|_| "bad year_id")?;
    let playoff = parse_bool(&get(rec, "is_playoffs"));
    let team = get(rec, "team_id");
    let opp = get(rec, "opp_id");
    if team.is_empty() || opp.is_empty() || team == opp {
        return Err("bad team codes".into());
    }
    let loc = get(rec, "game_location").to_ascii_uppercase();
    let neutral = loc == "N";
    let team_home = loc != "A";
    let pts = parse_i32(&get(rec, "pts"));
    let opp_pts = parse_i32(&get(rec, "opp_pts"));
    let elo_i = parse_f64(&get(rec, "elo_i")).ok_or("bad elo_i")?;
    let elo_n = parse_f64(&get(rec, "elo_n")).unwrap_or(elo_i);
    let opp_elo_i = parse_f64(&get(rec, "opp_elo_i")).ok_or("bad opp_elo_i")?;
    let opp_elo_n = parse_f64(&get(rec, "opp_elo_n")).unwrap_or(opp_elo_i);
    let forecast = parse_f64(&get(rec, "forecast"));
    let key = (date, team.clone().min(opp.clone()), team.clone().max(opp.clone()));
    let (_, kept) = seen.entry(key).or_insert((0, 0));
    let seq = *kept;
    *kept += 1;
    let (home, away, hs, as_, hpre, apre, hpost, apost, hprob) = if team_home {
        (team, opp, pts, opp_pts, elo_i, opp_elo_i, elo_n, opp_elo_n, forecast)
    } else {
        (opp, team, opp_pts, pts, opp_elo_i, elo_i, opp_elo_n, elo_n, forecast.map(|f| 1.0 - f))
    };
    Ok(Some(ParsedGame {
        season,
        date,
        seq: seq as i16,
        home,
        away,
        neutral,
        playoff,
        home_score: hs,
        away_score: as_,
        home_elo_pre: hpre,
        away_elo_pre: apre,
        home_elo_post: hpost,
        away_elo_post: apost,
        home_win_prob: hprob,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAINE: &str = "date,season,neutral,playoff,team1,team2,elo1_pre,elo2_pre,elo_prob1,elo_prob2,elo1_post,elo2_post,score1,score2,is_home
2016-06-19,2016,0,TRUE,GSW,CLE,1764.828667,1749.839437,0.659694076,0.340305924,1755.639,1759.029104,89,93,1
2016-06-19,2016,0,TRUE,CLE,GSW,1749.839437,1764.828667,0.340305924,0.659694076,1759.029104,1755.639,93,89,0
2020-08-01,2020,1,,LAL,TOR,1700,1650,0.57,0.43,1690,1660,92,107,0
2020-08-01,2020,1,,TOR,LAL,1650,1700,0.43,0.57,1660,1690,107,92,0
bad-date,2016,0,,BOS,NYK,1500,1500,0.5,0.5,1500,1500,1,2,1
";

    #[test]
    fn parses_two_perspective_layout() {
        let r = parse(PAINE.as_bytes()).unwrap();
        assert_eq!(r.format, "fivethirtyeight_nba_elo");
        assert_eq!(r.rows_read, 5);
        assert_eq!(r.rows_rejected, 1);
        assert_eq!(r.games.len(), 2);
        let g = &r.games[0];
        assert_eq!(g.home, "GSW");
        assert_eq!(g.away, "CLE");
        assert!(g.playoff);
        assert_eq!(g.home_score, Some(89));
        assert_eq!(g.seq, 0);
        let n = &r.games[1];
        assert!(n.neutral);
        assert_eq!(n.home, "LAL");
    }

    const FTE: &str = "date,season,neutral,playoff,team1,team2,elo1_pre,elo2_pre,elo_prob1,elo_prob2,elo1_post,elo2_post,carm-elo1_pre,score1,score2
2019-10-22,2020,0,,TOR,NOP,1600,1500,0.7,0.3,1605,1495,,130,122
2019-10-22,2020,0,,TOR,NOP,1600,1500,0.7,0.3,1605,1495,,130,122
2020-10-15,2020,0,,TOR,NOP,1600,1500,0.7,0.3,,,,,
";

    #[test]
    fn parses_single_row_layout_with_double_header_and_future_games() {
        let r = parse(FTE.as_bytes()).unwrap();
        assert_eq!(r.games.len(), 3);
        assert_eq!(r.games[0].seq, 0);
        assert_eq!(r.games[1].seq, 1);
        assert_eq!(r.games[2].home_score, None);
        assert_eq!(r.games[2].home_elo_post, 1600.0);
    }

    const OLD: &str = "gameorder,game_id,lg_id,_iscopy,year_id,date_game,seasongame,is_playoffs,team_id,fran_id,pts,elo_i,elo_n,win_equiv,opp_id,opp_fran,opp_pts,opp_elo_i,opp_elo_n,game_location,game_result,forecast,notes
1,194611010TRH,NBA,0,1947,11/1/1946,1,0,TRH,Huskies,66,1300,1293.2767,40.29483,NYK,Knicks,68,1300,1306.7233,H,L,0.64006501,
1,194611010TRH,NBA,1,1947,11/1/1946,1,0,NYK,Knicks,68,1300,1306.7233,41.70517,TRH,Huskies,66,1300,1293.2767,A,W,0.35993499,
2,194611020CHS,NBA,0,1947,11/2/1946,1,0,CHS,Stags,63,1300,1309.6521,42.01208,NYK,Knicks,47,1306.7233,1297.0712,A,W,0.63,
";

    #[test]
    fn parses_nbaallelo_layout() {
        let r = parse(OLD.as_bytes()).unwrap();
        assert_eq!(r.format, "fivethirtyeight_nbaallelo");
        assert_eq!(r.games.len(), 2);
        assert_eq!(r.games[0].home, "TRH");
        assert_eq!(r.games[0].away_score, Some(68));
        // second game: CHS was away, so NYK is home
        assert_eq!(r.games[1].home, "NYK");
        assert_eq!(r.games[1].home_score, Some(47));
        assert!((r.games[1].home_win_prob.unwrap() - 0.37).abs() < 1e-9);
    }

    #[test]
    fn rejects_unknown_layout() {
        let r = parse("a,b,c\n1,2,3\n".as_bytes());
        assert!(matches!(r, Err(ParseError::UnknownFormat(_))));
    }

    #[test]
    fn handles_garbage_gracefully() {
        let r = parse("date,season,neutral,playoff,team1,team2,elo1_pre,elo2_pre,score1,score2\n\"unterminated,2016\n".as_bytes());
        // either a CSV error on the row or a rejection; never a panic
        if let Ok(rep) = r { assert_eq!(rep.games.len(), 0) }
    }
}
