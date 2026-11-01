//! A sportsbook's prices read as chances: an American moneyline as the probability it
//! implies, and a game's two moneylines with the bookmaker's margin taken out.
//!
//! A moneyline of -150 risks 150 to win 100, so it implies 150 / 250 = 60%; +130 wins 130
//! on 100 and implies 100 / 230 = 43.5%. The two sides of a game imply more than 100% in
//! total: the excess is the bookmaker's margin. Scaling both to a sum of 100% gives the
//! market's chances without it, the figures to set beside the model's.

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

#[cfg(test)]
mod tests {
    use super::*;

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
