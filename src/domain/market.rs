//! A sportsbook's prices read as chances, and the model's chances read as bets.
//!
//! A moneyline of -150 risks 150 to win 100, so it implies 150 / 250 = 60%; +130 wins 130
//! on 100 and implies 100 / 230 = 43.5%. The two sides of a game imply more than 100% in
//! total: the excess is the bookmaker's margin. Scaling both to a sum of 100% gives the
//! market's chances without it, the figures to set beside the model's.
//!
//! The other way round, a bet at a posted price returns, on average, the model's chance
//! of winning times the payout less the chance of losing; the margin is paid inside the
//! price. A bet whose expected return clears the reader's bar is an edge. For a spread,
//! the chance of covering comes from the model's expected margin and the scatter of real
//! margins around expected ones, measured from the played games.

use serde::Serialize;

use crate::domain::worth::normal_cdf;

/// The bar an expected return must clear for a bet to be flagged, as a share of the
/// stake: the maintainer's choice, adjustable per request.
pub const DEFAULT_MIN_RETURN: f64 = 0.05;

/// The probability an American moneyline implies, margin included; none for a value
/// between -100 and +100 (no such line exists) or one that is not finite.
pub fn implied_probability(odds: f64) -> Option<f64> {
    if !odds.is_finite() {
        return None;
    }
    if odds <= -100.0 {
        Some(-odds / (-odds + 100.0))
    } else if odds >= 100.0 {
        Some(100.0 / (odds + 100.0))
    } else {
        None
    }
}

/// A game's two moneylines read as chances without the bookmaker's margin.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FairChances {
    pub home: f64,
    pub away: f64,
    /// The implied chances' excess over 100%: 0.046 for a 4.6% margin.
    pub margin: f64,
}

/// The home and away chances the two moneylines imply, scaled to add up to 1. None when a
/// line is not a valid moneyline.
pub fn without_margin(home_odds: f64, away_odds: f64) -> Option<FairChances> {
    let home = implied_probability(home_odds)?;
    let away = implied_probability(away_odds)?;
    let total = home + away;
    Some(FairChances {
        home: home / total,
        away: away / total,
        margin: total - 1.0,
    })
}

/// A field of outright prices read as chances without the bookmaker's margin: the title
/// futures of every team.
#[derive(Debug, Clone, PartialEq)]
pub struct FairField {
    /// One chance per price given, in the same order, adding up to 1.
    pub chances: Vec<f64>,
    /// The implied chances' excess over 100%: 0.35 for a 35% margin, usual on futures.
    pub margin: f64,
}

/// The chances a field of American prices implies, each scaled by the same factor so they
/// add up to 1 (the proportional rule, as for a game's two moneylines). None when fewer
/// than two prices are given or any is not a valid line. The rule leaves the long shots
/// a little high, since a bookmaker's margin sits mostly on them; it is kept for being
/// the one rule used throughout, and the margin is reported beside the chances.
pub fn fair_field(prices: &[f64]) -> Option<FairField> {
    if prices.len() < 2 {
        return None;
    }
    let implied: Vec<f64> = prices
        .iter()
        .map(|p| implied_probability(*p))
        .collect::<Option<_>>()?;
    let total: f64 = implied.iter().sum();
    if total <= 0.0 || !total.is_finite() {
        return None;
    }
    Some(FairField {
        chances: implied.iter().map(|p| p / total).collect(),
        margin: total - 1.0,
    })
}

/// Which side of a game a bet or a lean takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Side {
    Home,
    Away,
}

/// What a bet is on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BetKind {
    Moneyline,
    Spread,
}

/// The units a one-unit bet at American `odds` wins when it wins: +150 returns 1.5, -150
/// two thirds. None for a value no line has.
pub fn payout(odds: f64) -> Option<f64> {
    implied_probability(odds)?;
    Some(if odds >= 0.0 {
        odds / 100.0
    } else {
        100.0 / -odds
    })
}

/// The expected return of a one-unit bet, in units: the chance of winning times the
/// payout, less the chance of losing; a push returns the stake and counts for nothing.
pub fn expected_return(p_win: f64, p_push: f64, odds: f64) -> Option<f64> {
    let pay = payout(odds)?;
    let p_lose = (1.0 - p_win - p_push).max(0.0);
    Some(p_win * pay - p_lose)
}

/// The chance the home side covers its spread, and the chance of a push, when the real
/// home margin scatters normally around the model's expected margin with standard
/// deviation `sigma`. Margins are whole points and never zero (the game plays on until a
/// side leads), so the normal's mass at zero is taken out and the rest rescaled. The home
/// side covers a spread `s` when the margin is at least the first whole number above
/// `-s`, and pushes when it equals `-s`, which only a whole-number line other than zero
/// allows.
pub fn cover_chances(home_margin: f64, home_spread: f64, sigma: f64) -> Option<(f64, f64)> {
    if !(home_margin.is_finite() && home_spread.is_finite() && sigma.is_finite() && sigma > 0.0) {
        return None;
    }
    let at_least = |k: f64| 1.0 - normal_cdf((k - 0.5 - home_margin) / sigma);
    let mass = |k: f64| at_least(k) - at_least(k + 1.0);
    let scale = 1.0 - mass(0.0);
    if scale <= 0.0 {
        return None;
    }
    let must_beat = -home_spread;
    let cover_from = must_beat.floor() + 1.0;
    let mut p_cover = at_least(cover_from);
    if cover_from <= 0.0 {
        p_cover -= mass(0.0);
    }
    let p_push = if must_beat.fract() == 0.0 && must_beat != 0.0 {
        mass(must_beat)
    } else {
        0.0
    };
    Some((
        (p_cover / scale).clamp(0.0, 1.0),
        (p_push / scale).clamp(0.0, 1.0),
    ))
}

/// A sportsbook's lines on a game, as the home side.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Lines {
    pub home_moneyline: Option<f64>,
    pub away_moneyline: Option<f64>,
    pub home_spread: Option<f64>,
    pub home_spread_odds: Option<f64>,
    pub away_spread_odds: Option<f64>,
}

/// A bet whose expected return at the posted price clears the bar.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Edge {
    pub side: Side,
    pub kind: BetKind,
    /// The posted price, and for a spread the side's line (positive when it gets points).
    pub price: f64,
    pub line: Option<f64>,
    /// The model's chance the bet wins, its chance of a push, and the chance the price
    /// implies with the bookmaker's margin in it: the bet pays when the first beats the
    /// last by enough.
    pub p_model: f64,
    pub p_push: f64,
    pub p_price: f64,
    /// Units per unit staked, on average, if the model's chance is right.
    pub expected_return: f64,
}

/// A spread bet without a price is taken at the usual -110.
pub const USUAL_SPREAD_PRICE: f64 = -110.0;

/// The bets on a game whose expected return reaches `min_return`, best first: each side's
/// moneyline at the model's chance, and each side of the spread at the chance of covering
/// given `sigma`, the scatter of real margins.
pub fn edges(
    p_home: f64,
    home_margin: f64,
    lines: &Lines,
    sigma: f64,
    min_return: f64,
) -> Vec<Edge> {
    let mut out = Vec::new();
    if !(0.0..=1.0).contains(&p_home) {
        return out;
    }
    let mut consider = |side, kind, price: f64, line, p_win: f64, p_push: f64| {
        if let (Some(r), Some(p_price)) = (
            expected_return(p_win, p_push, price),
            implied_probability(price),
        ) {
            if r >= min_return {
                out.push(Edge {
                    side,
                    kind,
                    price,
                    line,
                    p_model: p_win,
                    p_push,
                    p_price,
                    expected_return: r,
                });
            }
        }
    };
    if let Some(price) = lines.home_moneyline {
        consider(Side::Home, BetKind::Moneyline, price, None, p_home, 0.0);
    }
    if let Some(price) = lines.away_moneyline {
        consider(
            Side::Away,
            BetKind::Moneyline,
            price,
            None,
            1.0 - p_home,
            0.0,
        );
    }
    if let Some(s) = lines.home_spread {
        if let Some((p_home_covers, p_push)) = cover_chances(home_margin, s, sigma) {
            let home_price = lines.home_spread_odds.unwrap_or(USUAL_SPREAD_PRICE);
            let away_price = lines.away_spread_odds.unwrap_or(USUAL_SPREAD_PRICE);
            consider(
                Side::Home,
                BetKind::Spread,
                home_price,
                Some(s),
                p_home_covers,
                p_push,
            );
            let p_away_covers = (1.0 - p_home_covers - p_push).max(0.0);
            consider(
                Side::Away,
                BetKind::Spread,
                away_price,
                Some(-s),
                p_away_covers,
                p_push,
            );
        }
    }
    out.sort_by(|a, b| b.expected_return.total_cmp(&a.expected_return));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_field_of_prices_gives_chances_that_add_up_to_one_with_the_margin_beside() {
        // +300, +400, +900 and +1900: 25% + 20% + 10% + 5% = 60%, no margin to take out.
        let f = fair_field(&[300.0, 400.0, 900.0, 1900.0]).unwrap();
        assert!((f.chances.iter().sum::<f64>() - 1.0).abs() < 1e-12);
        assert!((f.chances[0] - 0.25 / 0.6).abs() < 1e-12);
        assert!((f.margin - (0.6 - 1.0)).abs() < 1e-12);
        // A real field carries a margin: -110 and +100 twice imply 52.4% and 50% twice.
        let f = fair_field(&[-110.0, 100.0, 100.0]).unwrap();
        assert!(f.margin > 0.5 && f.margin < 0.53);
        assert!(f.chances[0] > f.chances[1] && f.chances[1] == f.chances[2]);
        assert_eq!(fair_field(&[300.0]), None, "one price is no field");
        assert_eq!(fair_field(&[300.0, 50.0]), None, "+50 is no line");
        assert_eq!(fair_field(&[]), None);
    }

    #[test]
    fn a_bet_returns_its_chance_times_the_payout_less_the_chance_of_losing() {
        assert_eq!(payout(150.0), Some(1.5));
        assert!((payout(-150.0).unwrap() - 2.0 / 3.0).abs() < 1e-12);
        assert_eq!(payout(50.0), None);
        // OKC +150 at a model 52%: 0.52 × 1.5 − 0.48 = +0.30 a unit.
        assert!((expected_return(0.52, 0.0, 150.0).unwrap() - 0.30).abs() < 1e-12);
        // At the price's own implied chance the return is zero: the price is fair to itself.
        let p = implied_probability(-135.0).unwrap();
        assert!(expected_return(p, 0.0, -135.0).unwrap().abs() < 1e-12);
        // A push takes its share out of the losing side only.
        assert!((expected_return(0.5, 0.1, -110.0).unwrap() - (0.5 / 1.1 - 0.4)).abs() < 1e-12);
    }

    #[test]
    fn covering_a_spread_follows_the_margin_scatter_and_whole_lines_can_push() {
        // A pick'em (spread 0, model margin 0): no push, since a game cannot end level,
        // and each side covers half the time.
        let (cover, push) = cover_chances(0.0, 0.0, 13.0).unwrap();
        assert_eq!(push, 0.0);
        assert!((cover - 0.5).abs() < 1e-9, "{cover}");
        // A whole line away from zero pushes about three percent of the time at a
        // 13-point scatter, and cover, push and lose add up to one.
        let (cover, push) = cover_chances(0.0, -3.0, 13.0).unwrap();
        assert!(push > 0.025 && push < 0.035, "{push}");
        assert!(cover < 0.5 && cover + push < 1.0);
        // A half-point line never pushes; the favourite laying 2.5 with a 5-point edge
        // covers more often than not.
        let (cover, push) = cover_chances(5.0, -2.5, 13.0).unwrap();
        assert_eq!(push, 0.0);
        assert!(cover > 0.55 && cover < 0.6, "{cover}");
        // Both sides of a half-point line add up to one once the zero margin is out.
        let (home, _) = cover_chances(1.0, -0.5, 13.0).unwrap();
        let (away, _) = cover_chances(-1.0, -0.5, 13.0).unwrap();
        assert!((home + (1.0 - home) - 1.0).abs() < 1e-12 && home > 0.5 && away < 0.5);
        // A bigger scatter pulls the chance toward a half; no scatter is no answer.
        let (wide, _) = cover_chances(5.0, -2.5, 30.0).unwrap();
        assert!(wide < cover && wide > 0.5);
        assert!(cover_chances(5.0, -2.5, 0.0).is_none());
    }

    #[test]
    fn the_edges_are_the_bets_that_clear_the_bar_best_first() {
        // Sacramento +340 with the model at 29%: +0.28 a unit; Dallas -430 at 71%: a loss.
        // The spread: the model's margin is DAL by 5.6 against a 9.5 line, so SAC covers.
        let lines = Lines {
            home_moneyline: Some(-430.0),
            away_moneyline: Some(340.0),
            home_spread: Some(-9.5),
            home_spread_odds: Some(-118.0),
            away_spread_odds: Some(-104.0),
        };
        let e = edges(0.71, 5.6, &lines, 13.0, 0.05);
        assert_eq!(e.len(), 2, "{e:?}");
        assert_eq!((e[0].side, e[0].kind), (Side::Away, BetKind::Moneyline));
        assert!((e[0].expected_return - (0.29 * 3.4 - 0.71)).abs() < 1e-12);
        assert_eq!(
            (e[1].side, e[1].kind, e[1].line),
            (Side::Away, BetKind::Spread, Some(9.5))
        );
        assert!(
            e[1].p_model > 0.6
                && e[1].expected_return > 0.05
                && e[1].expected_return < e[0].expected_return
        );
        // A higher bar drops the spread; a bar of zero keeps any positive return.
        assert_eq!(edges(0.71, 5.6, &lines, 13.0, 0.25).len(), 1);
        assert!(edges(0.71, 5.6, &lines, 13.0, 0.0).len() >= 2);
        // Without a spread price the usual -110 is taken; without lines there is nothing.
        let bare = Lines {
            home_spread: Some(-9.5),
            ..Lines::default()
        };
        assert_eq!(
            edges(0.71, 5.6, &bare, 13.0, 0.0)[0].price,
            USUAL_SPREAD_PRICE
        );
        assert!(edges(0.5, 0.0, &Lines::default(), 13.0, 0.0).is_empty());
        assert!(edges(1.5, 0.0, &lines, 13.0, 0.0).is_empty());
    }

    #[test]
    fn a_moneyline_implies_its_probability() {
        assert!((implied_probability(-150.0).unwrap() - 0.6).abs() < 1e-12);
        assert!((implied_probability(130.0).unwrap() - 100.0 / 230.0).abs() < 1e-12);
        assert_eq!(implied_probability(100.0), Some(0.5));
        assert_eq!(implied_probability(-100.0), Some(0.5));
        for bad in [0.0, 50.0, -99.0, f64::NAN, f64::INFINITY] {
            assert_eq!(implied_probability(bad), None, "{bad}");
        }
    }

    #[test]
    fn the_margin_comes_out_and_the_chances_add_up_to_one() {
        // FanDuel's Warriors -130, Clippers +108: 56.5% and 48.1%, a 4.6% margin.
        let f = without_margin(-130.0, 108.0).unwrap();
        assert!((f.home + f.away - 1.0).abs() < 1e-12);
        assert!((f.margin - (130.0 / 230.0 + 100.0 / 208.0 - 1.0)).abs() < 1e-12);
        assert!((f.home - 0.540).abs() < 0.001, "{}", f.home);
        // A line without a margin keeps its chances.
        let even = without_margin(100.0, 100.0).unwrap();
        assert_eq!((even.home, even.away, even.margin), (0.5, 0.5, 0.0));
        assert!(without_margin(-130.0, 50.0).is_none());
        // Extreme lines stay finite and add up; two plus lines give a negative margin.
        let long = without_margin(-100_000.0, 50_000.0).unwrap();
        assert!(long.home > 0.99 && (long.home + long.away - 1.0).abs() < 1e-12);
        let generous = without_margin(110.0, 110.0).unwrap();
        assert!(generous.margin < 0.0 && (generous.home - 0.5).abs() < 1e-12);
    }
}
