//! The model against the market: each game's model numbers and sportsbook lines as they
//! stood just before tip-off, scored once the game is played.
//!
//! A snapshot holds what the Tonight panel showed for a game: the model's chance and
//! expected margin at the ratings the teams played at, and the sportsbook's spread with
//! its prices, its moneylines and the chance they imply with the margin taken out. The last
//! snapshot taken before the game starts stands for it. After the result, each game says
//! who was closer on the chance (the Brier score and the log loss), which side the model
//! leaned to against the market's price, and how that lean did at the sportsbook's prices,
//! on the moneyline and against the spread. The scores are kept per model version, so a
//! change to the model shows as a new row rather than blending into the old record.

use std::collections::BTreeMap;

use chrono::{DateTime, NaiveDate, Utc};
use serde::Serialize;

use crate::domain::ids::SeasonYear;
use crate::domain::market;

/// The parts of the model a snapshot's numbers come from, joined: a change to any shows as
/// a new version in the record.
pub fn tonight_model_version() -> String {
    [
        crate::domain::full_strength::FULL_STRENGTH_MODEL,
        crate::domain::worth::WORTH_POOLING_VERSION,
        crate::domain::worth::BOX_CALIBRATION_VERSION,
        crate::domain::bpm::BOX_MODEL_VERSION,
        crate::domain::worth::MARGIN_MODEL_VERSION,
        crate::domain::status::STATUS_MODEL_VERSION,
        crate::domain::spell::SPELL_MODEL_VERSION,
        crate::domain::rookie::DRAFT_MODEL_VERSION,
    ]
    .join(" ")
}

/// One game as the model and the sportsbook priced it before tip-off. The home side is
/// the stored game's; a spread is the home team's, negative when it gives points.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Snapshot {
    pub season: SeasonYear,
    pub game_date: NaiveDate,
    pub home_code: String,
    pub away_code: String,
    pub captured_at: DateTime<Utc>,
    /// When the game was due to start, as the market source gives it.
    pub start_time: Option<DateTime<Utc>>,
    pub model_version: String,
    pub app_version: String,
    pub home_advantage: f64,
    /// The full-strength ratings, the stored ones, and the penalties of the night.
    pub home_rating: f64,
    pub away_rating: f64,
    pub home_stored: f64,
    pub away_stored: f64,
    pub home_penalty: f64,
    pub away_penalty: f64,
    /// The model's home chance at full strength and on the night, and its expected home
    /// margin on the night in points.
    pub p_home_full: f64,
    pub p_home_model: f64,
    pub home_margin_model: f64,
    /// The sportsbook and the source of its lines.
    pub provider: String,
    pub source: String,
    pub home_spread: Option<f64>,
    pub home_spread_odds: Option<f64>,
    pub away_spread_odds: Option<f64>,
    pub home_moneyline: Option<f64>,
    pub away_moneyline: Option<f64>,
    pub total: Option<f64>,
    /// The market's home chance with its margin taken out, and that margin.
    pub p_home_market: Option<f64>,
    pub book_margin: Option<f64>,
}

/// A played game's result, by the home side.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Outcome {
    pub home_score: i32,
    pub away_score: i32,
}

/// Which side of a game a bet or a lean takes (`domain::market`).
pub use crate::domain::market::Side;

/// How one bet went.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Bet {
    Won,
    Lost,
    Push,
}

/// One snapshot scored against its result.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Scored {
    pub snapshot: Snapshot,
    pub home_score: i32,
    pub away_score: i32,
    pub home_won: bool,
    /// Squared error of each home chance against the result, lower is better.
    pub brier_model: f64,
    pub brier_market: Option<f64>,
    /// The side whose chance the model put above the market's, and how a one-unit bet on
    /// it at the sportsbook's moneyline went, with its return in units.
    pub lean: Option<Side>,
    pub lean_result: Option<Bet>,
    pub lean_units: Option<f64>,
    /// The side the model's margin took against the spread, and how that bet went at the
    /// spread's price.
    pub spread_side: Option<Side>,
    pub spread_result: Option<Bet>,
    pub spread_units: Option<f64>,
    /// The bets the edge rule flagged on the stored line, with how each went.
    pub flagged: Vec<FlaggedBet>,
}

/// A flagged bet scored: the edge as it stood, how it went, and its units.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct FlaggedBet {
    pub edge: market::Edge,
    pub result: Bet,
    pub units: f64,
}

/// The return of a one-unit bet at American `odds` that won: +150 returns 1.5, -150 two
/// thirds. A value no line has pays nothing.
fn win_units(odds: f64) -> f64 {
    market::payout(odds).unwrap_or(0.0)
}

fn units(bet: Bet, odds: f64) -> f64 {
    match bet {
        Bet::Won => win_units(odds),
        Bet::Lost => -1.0,
        Bet::Push => 0.0,
    }
}

/// A chance held off zero and one, so the log loss stays finite.
fn clamp_p(p: f64) -> f64 {
    p.clamp(1e-6, 1.0 - 1e-6)
}

fn log_loss(p: f64, won: bool) -> f64 {
    let p = clamp_p(p);
    -(if won { p.ln() } else { (1.0 - p).ln() })
}

/// Score one snapshot against its result. A tie, which the sport does not have, scores
/// nothing. `sigma` and `min_return` are the edge rule's: the scatter of real margins and
/// the bar a bet's expected return must clear to be flagged.
pub fn score(snapshot: Snapshot, outcome: Outcome, sigma: f64, min_return: f64) -> Option<Scored> {
    if outcome.home_score == outcome.away_score {
        return None;
    }
    let home_won = outcome.home_score > outcome.away_score;
    let y = if home_won { 1.0 } else { 0.0 };
    let brier_model = (snapshot.p_home_model - y).powi(2);
    let brier_market = snapshot.p_home_market.map(|p| (p - y).powi(2));
    // The lean: the side whose chance the model rates above the market's.
    let lean = snapshot.p_home_market.and_then(|m| {
        let d = snapshot.p_home_model - m;
        if d > 0.0 {
            Some(Side::Home)
        } else if d < 0.0 {
            Some(Side::Away)
        } else {
            None
        }
    });
    let lean_odds = lean.and_then(|s| match s {
        Side::Home => snapshot.home_moneyline,
        Side::Away => snapshot.away_moneyline,
    });
    let lean_result = lean.zip(lean_odds).map(|(s, _)| {
        if (s == Side::Home) == home_won {
            Bet::Won
        } else {
            Bet::Lost
        }
    });
    let lean_units = lean_result.zip(lean_odds).map(|(b, o)| units(b, o));
    // Against the spread: the model expects a home margin of m; the market's line asks
    // the home side to win by more than -spread.
    let margin = f64::from(outcome.home_score - outcome.away_score);
    let spread_side = snapshot.home_spread.and_then(|s| {
        let edge = snapshot.home_margin_model + s;
        if edge > 0.0 {
            Some(Side::Home)
        } else if edge < 0.0 {
            Some(Side::Away)
        } else {
            None
        }
    });
    let spread_result = spread_side.zip(snapshot.home_spread).map(|(side, s)| {
        let covered = margin + s;
        let home_bet = side == Side::Home;
        if covered == 0.0 {
            Bet::Push
        } else if (covered > 0.0) == home_bet {
            Bet::Won
        } else {
            Bet::Lost
        }
    });
    let spread_odds = spread_side.and_then(|s| match s {
        Side::Home => snapshot.home_spread_odds,
        Side::Away => snapshot.away_spread_odds,
    });
    // A spread bet without a price is taken at the usual -110.
    let spread_units =
        spread_result.map(|b| units(b, spread_odds.unwrap_or(market::USUAL_SPREAD_PRICE)));
    // The flagged bets: the edge rule on the stored line, each scored as it was priced.
    let lines = market::Lines {
        home_moneyline: snapshot.home_moneyline,
        away_moneyline: snapshot.away_moneyline,
        home_spread: snapshot.home_spread,
        home_spread_odds: snapshot.home_spread_odds,
        away_spread_odds: snapshot.away_spread_odds,
    };
    let flagged = market::edges(
        snapshot.p_home_model,
        snapshot.home_margin_model,
        &lines,
        sigma,
        min_return,
    )
    .into_iter()
    .map(|edge| {
        let home_bet = edge.side == Side::Home;
        let result = match edge.kind {
            market::BetKind::Moneyline => {
                if home_bet == home_won {
                    Bet::Won
                } else {
                    Bet::Lost
                }
            }
            market::BetKind::Spread => {
                // The home side's line, whichever side the bet is on.
                let covered = margin
                    + if home_bet {
                        edge.line.unwrap_or(0.0)
                    } else {
                        -edge.line.unwrap_or(0.0)
                    };
                if covered == 0.0 {
                    Bet::Push
                } else if (covered > 0.0) == home_bet {
                    Bet::Won
                } else {
                    Bet::Lost
                }
            }
        };
        FlaggedBet {
            edge,
            result,
            units: units(result, edge.price),
        }
    })
    .collect();
    Some(Scored {
        home_score: outcome.home_score,
        away_score: outcome.away_score,
        home_won,
        brier_model,
        brier_market,
        lean,
        lean_result,
        lean_units,
        spread_side,
        spread_result,
        spread_units,
        flagged,
        snapshot,
    })
}

/// A group of scored games summed: how the model and the market did, and the model's
/// bets against the market.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Summary {
    pub games: u32,
    /// Mean Brier score and log loss of each, over the games the market priced.
    pub priced: u32,
    pub brier_model: f64,
    pub brier_market: f64,
    pub log_loss_model: f64,
    pub log_loss_market: f64,
    /// The model's side on the moneyline: wins, losses, units and return per unit staked.
    pub lean_won: u32,
    pub lean_lost: u32,
    pub lean_units: f64,
    pub lean_return: f64,
    /// The model's side against the spread.
    pub spread_won: u32,
    pub spread_lost: u32,
    pub spread_push: u32,
    pub spread_units: f64,
    pub spread_return: f64,
    /// The bets the edge rule flagged: how many, how they went, and the return per bet.
    pub flagged_won: u32,
    pub flagged_lost: u32,
    pub flagged_push: u32,
    pub flagged_units: f64,
    pub flagged_return: f64,
}

/// Sum the games given. The Brier and log loss means are over the games with a market
/// chance, so the two columns compare the same games.
pub fn summarize<'a>(games: impl IntoIterator<Item = &'a Scored>) -> Summary {
    let mut s = Summary::default();
    for g in games {
        s.games += 1;
        if let (Some(bm), Some(pm)) = (g.brier_market, g.snapshot.p_home_market) {
            s.priced += 1;
            s.brier_model += g.brier_model;
            s.brier_market += bm;
            s.log_loss_model += log_loss(g.snapshot.p_home_model, g.home_won);
            s.log_loss_market += log_loss(pm, g.home_won);
        }
        match g.lean_result {
            Some(Bet::Won) => s.lean_won += 1,
            Some(Bet::Lost) => s.lean_lost += 1,
            _ => {}
        }
        s.lean_units += g.lean_units.unwrap_or(0.0);
        match g.spread_result {
            Some(Bet::Won) => s.spread_won += 1,
            Some(Bet::Lost) => s.spread_lost += 1,
            Some(Bet::Push) => s.spread_push += 1,
            None => {}
        }
        s.spread_units += g.spread_units.unwrap_or(0.0);
        for f in &g.flagged {
            match f.result {
                Bet::Won => s.flagged_won += 1,
                Bet::Lost => s.flagged_lost += 1,
                Bet::Push => s.flagged_push += 1,
            }
            s.flagged_units += f.units;
        }
    }
    if s.priced > 0 {
        let n = f64::from(s.priced);
        s.brier_model /= n;
        s.brier_market /= n;
        s.log_loss_model /= n;
        s.log_loss_market /= n;
    }
    let lean_bets = s.lean_won + s.lean_lost;
    if lean_bets > 0 {
        s.lean_return = s.lean_units / f64::from(lean_bets);
    }
    let spread_bets = s.spread_won + s.spread_lost + s.spread_push;
    if spread_bets > 0 {
        s.spread_return = s.spread_units / f64::from(spread_bets);
    }
    let flagged_bets = s.flagged_won + s.flagged_lost + s.flagged_push;
    if flagged_bets > 0 {
        s.flagged_return = s.flagged_units / f64::from(flagged_bets);
    }
    s
}

/// A summary with the key it groups by: a model version or a month.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Group {
    pub key: String,
    #[serde(flatten)]
    pub summary: Summary,
}

/// The scored games grouped by `key`, in key order.
pub fn group_by(games: &[Scored], key: impl Fn(&Scored) -> String) -> Vec<Group> {
    let mut groups: BTreeMap<String, Vec<&Scored>> = BTreeMap::new();
    for g in games {
        groups.entry(key(g)).or_default().push(g);
    }
    groups
        .into_iter()
        .map(|(key, gs)| Group {
            key,
            summary: summarize(gs),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snap(p_model: f64, p_market: Option<f64>, margin: f64, spread: Option<f64>) -> Snapshot {
        Snapshot {
            season: SeasonYear::of(2027),
            game_date: NaiveDate::from_ymd_opt(2026, 11, 1).unwrap(),
            home_code: "GSW".into(),
            away_code: "LAC".into(),
            captured_at: DateTime::<Utc>::from_timestamp(0, 0).unwrap(),
            start_time: None,
            model_version: "m1".into(),
            app_version: "0.1.0".into(),
            home_advantage: 41.0,
            home_rating: 1497.0,
            away_rating: 1497.0,
            home_stored: 1497.0,
            away_stored: 1497.0,
            home_penalty: 58.0,
            away_penalty: 17.0,
            p_home_full: 0.56,
            p_home_model: p_model,
            home_margin_model: margin,
            provider: "DraftKings".into(),
            source: "espn".into(),
            home_spread: spread,
            home_spread_odds: Some(-105.0),
            away_spread_odds: Some(-115.0),
            home_moneyline: Some(-135.0),
            away_moneyline: Some(114.0),
            total: Some(219.5),
            p_home_market: p_market,
            book_margin: Some(0.04),
        }
    }

    #[test]
    fn a_game_scores_the_chances_and_the_models_bets_against_the_market() {
        // The model at a coin flip, the market at 55% for Golden State, -2.5; the
        // Clippers win by 4.
        let s = score(
            snap(0.50, Some(0.55), 0.0, Some(-2.5)),
            Outcome {
                home_score: 100,
                away_score: 104,
            },
            13.0,
            0.05,
        )
        .unwrap();
        assert!(!s.home_won);
        assert!((s.brier_model - 0.25).abs() < 1e-12);
        assert!((s.brier_market.unwrap() - 0.3025).abs() < 1e-12);
        // The model leaned to the Clippers at +114 and won 1.14 units.
        assert_eq!((s.lean, s.lean_result), (Some(Side::Away), Some(Bet::Won)));
        assert!((s.lean_units.unwrap() - 1.14).abs() < 1e-12);
        // Its margin of 0 took the Clippers +2.5 at -115; they covered.
        assert_eq!(
            (s.spread_side, s.spread_result),
            (Some(Side::Away), Some(Bet::Won))
        );
        assert!((s.spread_units.unwrap() - 100.0 / 115.0).abs() < 1e-12);
        // A favourite's win at -135 returns 100/135; a push returns nothing.
        let fav = score(
            snap(0.60, Some(0.55), 3.0, Some(-3.0)),
            Outcome {
                home_score: 110,
                away_score: 107,
            },
            13.0,
            0.05,
        )
        .unwrap();
        assert!((fav.lean_units.unwrap() - 100.0 / 135.0).abs() < 1e-12);
        assert_eq!(fav.spread_result, None);
        let push = score(
            snap(0.60, Some(0.55), 5.0, Some(-3.0)),
            Outcome {
                home_score: 110,
                away_score: 107,
            },
            13.0,
            0.05,
        )
        .unwrap();
        assert_eq!(
            (push.spread_side, push.spread_result, push.spread_units),
            (Some(Side::Home), Some(Bet::Push), Some(0.0))
        );
        // No market chance: no lean, no market score; a tie scores nothing.
        let bare = score(
            snap(0.6, None, 1.0, None),
            Outcome {
                home_score: 90,
                away_score: 80,
            },
            13.0,
            0.05,
        )
        .unwrap();
        assert_eq!(
            (bare.lean, bare.brier_market, bare.spread_side),
            (None, None, None)
        );
        assert!(score(
            snap(0.6, None, 1.0, None),
            Outcome {
                home_score: 90,
                away_score: 90
            },
            13.0,
            0.05,
        )
        .is_none());
    }

    #[test]
    fn the_flagged_bets_are_scored_as_priced_and_summed() {
        // The Clippers at +114 with the model at 50%: 0.5 × 1.14 − 0.5 = +0.07 a unit,
        // above a 5% bar; the model's margin of 0 against -2.5 at -115 for the Clippers:
        // they cover about 56% of the time, 0.56 × 0.87 − 0.44 = +0.05. Both won by 4.
        let s = score(
            snap(0.50, Some(0.55), 0.0, Some(-2.5)),
            Outcome {
                home_score: 100,
                away_score: 104,
            },
            13.0,
            0.05,
        )
        .unwrap();
        let kinds: Vec<_> = s
            .flagged
            .iter()
            .map(|f| (f.edge.side, f.edge.kind, f.result))
            .collect();
        assert_eq!(
            kinds,
            vec![
                (Side::Away, market::BetKind::Moneyline, Bet::Won),
                (Side::Away, market::BetKind::Spread, Bet::Won)
            ],
            "{:?}",
            s.flagged
        );
        assert!((s.flagged[0].units - 1.14).abs() < 1e-12);
        // A bar above both returns flags nothing; the summary counts and averages them.
        let none = score(
            snap(0.50, Some(0.55), 0.0, Some(-2.5)),
            Outcome {
                home_score: 100,
                away_score: 104,
            },
            13.0,
            0.5,
        )
        .unwrap();
        assert!(none.flagged.is_empty());
        let sum = summarize([&s, &none]);
        assert_eq!(
            (sum.flagged_won, sum.flagged_lost, sum.flagged_push),
            (2, 0, 0)
        );
        assert!(
            (sum.flagged_return - sum.flagged_units / 2.0).abs() < 1e-12 && sum.flagged_units > 2.0
        );
    }

    #[test]
    fn the_summary_compares_the_same_games_and_groups_by_version() {
        let win = Outcome {
            home_score: 100,
            away_score: 90,
        };
        let mut a = score(snap(0.7, Some(0.6), 4.0, Some(-2.0)), win, 13.0, 0.05).unwrap();
        let mut b = score(snap(0.4, Some(0.5), -1.0, Some(-2.0)), win, 13.0, 0.05).unwrap();
        let c = score(snap(0.6, None, 2.0, None), win, 13.0, 0.05).unwrap();
        a.snapshot.model_version = "m1".into();
        b.snapshot.model_version = "m2".into();
        let all = [a, b, c];
        let s = summarize(&all);
        assert_eq!((s.games, s.priced), (3, 2));
        // Brier over the two priced games: (0.09 + 0.36) / 2 against (0.16 + 0.25) / 2.
        assert!((s.brier_model - 0.225).abs() < 1e-12);
        assert!((s.brier_market - 0.205).abs() < 1e-12);
        // Lean: home won at -135 (game a), away lost (game b).
        assert_eq!((s.lean_won, s.lean_lost), (1, 1));
        assert!((s.lean_units - (100.0 / 135.0 - 1.0)).abs() < 1e-12);
        assert!((s.lean_return - s.lean_units / 2.0).abs() < 1e-12);
        let groups = group_by(&all, |g| g.snapshot.model_version.clone());
        let keys: Vec<&str> = groups.iter().map(|g| g.key.as_str()).collect();
        assert_eq!(keys, ["m1", "m2"]);
        assert_eq!(groups[0].summary.games, 2);
        assert!(!tonight_model_version().is_empty());
    }

    #[test]
    fn the_spread_bet_reads_covers_both_ways_and_the_log_loss_is_exact() {
        let result = |h: i32, a: i32| Outcome {
            home_score: h,
            away_score: a,
        };
        // Home -6.5, the model's margin 2: it takes the visitor, who loses by 4 and covers.
        let away_covers = score(
            snap(0.6, Some(0.7), 2.0, Some(-6.5)),
            result(104, 100),
            13.0,
            0.05,
        )
        .unwrap();
        assert_eq!(
            (away_covers.spread_side, away_covers.spread_result),
            (Some(Side::Away), Some(Bet::Won))
        );
        // Home +3.5, the model's margin 1: it takes the home side, which loses by 2 and
        // covers; losing by 5 it does not.
        let home_covers = score(
            snap(0.5, Some(0.4), 1.0, Some(3.5)),
            result(98, 100),
            13.0,
            0.05,
        )
        .unwrap();
        assert_eq!(
            (home_covers.spread_side, home_covers.spread_result),
            (Some(Side::Home), Some(Bet::Won))
        );
        assert!((home_covers.spread_units.unwrap() - 100.0 / 105.0).abs() < 1e-12);
        let home_fails = score(
            snap(0.5, Some(0.4), 1.0, Some(3.5)),
            result(95, 100),
            13.0,
            0.05,
        )
        .unwrap();
        assert_eq!(home_fails.spread_result, Some(Bet::Lost));
        // Log loss: -ln(0.6) for the model and -ln(0.7) for the market on a home win.
        let s = summarize([&away_covers]);
        assert!((s.log_loss_model - -(0.6f64.ln())).abs() < 1e-12);
        assert!((s.log_loss_market - -(0.7f64.ln())).abs() < 1e-12);
        assert!(tonight_model_version().contains(crate::domain::bpm::BOX_MODEL_VERSION));
        // The margin version names the Elo a point it was written for.
        let per_point = format!("{}", crate::domain::worth::ELO_PER_POINT as i64);
        assert!(crate::domain::worth::MARGIN_MODEL_VERSION.contains(&per_point));
    }
}
