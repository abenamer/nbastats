//! What a draft slot has been worth as a rookie. A player with no stored regular-season
//! lines has no worth, and a first-round pick is not nothing. Over the stored seasons, the
//! first-season box worth of every rookie is pooled by draft slot bracket into a mean, the
//! error of that mean and the spread across the rookies: the prior a rookie starts the
//! season with, and the figure his own box worth is shrunk toward while his minutes are
//! few. Pooled by bracket, never by player. The brackets, the minimum, the window and the
//! weight of the minutes are choices; the figures are measured.

use serde::Serialize;

/// Named in the availability answer.
pub const DRAFT_MODEL_VERSION: &str = "draft-prior-1.0";
/// A bracket with fewer rookies than this gives no prior.
pub const MIN_PLAYERS: usize = 10;
/// The minutes at which a rookie's own box worth and the bracket prior weigh the same;
/// about twenty games at twenty-five minutes.
pub const PRIOR_MINUTES: f64 = 500.0;

/// Where a player was taken in the draft.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DraftBracket {
    /// The first pick.
    FirstPick,
    /// Picks 2 to 5.
    Top5,
    /// Picks 6 to 14.
    Lottery,
    /// The rest of the first round.
    FirstRound,
    /// The second round and any later one.
    SecondRound,
    /// Not drafted, or the draft unknown.
    Undrafted,
}

impl DraftBracket {
    pub const ALL: [DraftBracket; 6] = [
        DraftBracket::FirstPick,
        DraftBracket::Top5,
        DraftBracket::Lottery,
        DraftBracket::FirstRound,
        DraftBracket::SecondRound,
        DraftBracket::Undrafted,
    ];

    /// The bracket of a draft round and overall pick, as the player feed gives them.
    pub fn of(round: Option<i32>, number: Option<i32>) -> DraftBracket {
        match (round, number) {
            (Some(r), _) if r >= 2 => DraftBracket::SecondRound,
            (Some(1), Some(1)) => DraftBracket::FirstPick,
            (Some(1), Some(n)) if (2..=5).contains(&n) => DraftBracket::Top5,
            (Some(1), Some(n)) if (6..=14).contains(&n) => DraftBracket::Lottery,
            (Some(1), _) => DraftBracket::FirstRound,
            _ => DraftBracket::Undrafted,
        }
    }

    /// The wider bracket that stands in when this one has too few rookies: the first pick
    /// alone can give one rookie a season, so picks 2 to 5 stand in for it, and so on
    /// through the first round. The second round and the undrafted have no stand-in.
    pub fn wider(self) -> Option<DraftBracket> {
        match self {
            DraftBracket::FirstPick => Some(DraftBracket::Top5),
            DraftBracket::Top5 => Some(DraftBracket::Lottery),
            DraftBracket::Lottery => Some(DraftBracket::FirstRound),
            _ => None,
        }
    }
}

/// One rookie's first season: his draft bracket and his box worth that season.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RookieSample {
    pub bracket: DraftBracket,
    pub worth_elo: f64,
}

/// What a bracket's rookies have been worth: the mean of their first-season box worth, the
/// error of that mean (the spread over the square root of the count), and the spread
/// itself (the sample standard deviation).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DraftPrior {
    pub bracket: DraftBracket,
    pub players: u32,
    pub mean_elo: f64,
    /// The error of the mean: what the pricing takes as the prior's error.
    pub se_elo: f64,
    /// The spread across the bracket's rookies, for the reader.
    pub sd_elo: f64,
}

/// The priors of every bracket with enough rookies.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DraftTable {
    pub model: &'static str,
    /// Rookies pooled, over every bracket.
    pub players: u32,
    pub priors: Vec<DraftPrior>,
}

impl DraftTable {
    pub fn fit(samples: &[RookieSample]) -> DraftTable {
        let priors = DraftBracket::ALL
            .iter()
            .filter_map(|&bracket| {
                let worths: Vec<f64> = samples
                    .iter()
                    .filter(|s| s.bracket == bracket)
                    .map(|s| s.worth_elo)
                    .collect();
                if worths.len() < MIN_PLAYERS {
                    return None;
                }
                let n = worths.len() as f64;
                let mean = worths.iter().sum::<f64>() / n;
                let var = worths.iter().map(|w| (w - mean).powi(2)).sum::<f64>() / (n - 1.0);
                Some(DraftPrior {
                    bracket,
                    players: worths.len() as u32,
                    mean_elo: mean,
                    se_elo: (var / n).sqrt(),
                    sd_elo: var.sqrt(),
                })
            })
            .collect();
        DraftTable {
            model: DRAFT_MODEL_VERSION,
            players: samples.len() as u32,
            priors,
        }
    }

    pub fn prior(&self, bracket: DraftBracket) -> Option<&DraftPrior> {
        self.priors.iter().find(|p| p.bracket == bracket)
    }

    /// The prior for a bracket, or the nearest wider bracket's when it has none; the prior
    /// names the bracket it comes from.
    pub fn prior_for(&self, bracket: DraftBracket) -> Option<&DraftPrior> {
        let mut b = Some(bracket);
        while let Some(current) = b {
            if let Some(p) = self.prior(current) {
                return Some(p);
            }
            b = current.wider();
        }
        None
    }
}

/// A rookie's worth with its error: the bracket prior, pulled toward his own box worth by
/// his minutes. At [`PRIOR_MINUTES`] the two weigh the same; with no minutes the prior
/// stands with the error of its mean as the error, which shrinks as the minutes grow.
pub fn shrink(prior: &DraftPrior, own: Option<(f64, f64)>) -> (f64, f64) {
    match own {
        Some((worth, minutes)) if minutes > 0.0 => {
            let w = minutes / (minutes + PRIOR_MINUTES);
            (
                w * worth + (1.0 - w) * prior.mean_elo,
                (1.0 - w) * prior.se_elo,
            )
        }
        _ => (prior.mean_elo, prior.se_elo),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pick_falls_in_its_bracket() {
        assert_eq!(DraftBracket::of(Some(1), Some(1)), DraftBracket::FirstPick);
        assert_eq!(DraftBracket::of(Some(1), Some(5)), DraftBracket::Top5);
        assert_eq!(DraftBracket::of(Some(1), Some(14)), DraftBracket::Lottery);
        assert_eq!(
            DraftBracket::of(Some(1), Some(15)),
            DraftBracket::FirstRound
        );
        assert_eq!(DraftBracket::of(Some(1), None), DraftBracket::FirstRound);
        assert_eq!(
            DraftBracket::of(Some(2), Some(31)),
            DraftBracket::SecondRound
        );
        assert_eq!(
            DraftBracket::of(Some(7), Some(140)),
            DraftBracket::SecondRound
        );
        assert_eq!(DraftBracket::of(None, None), DraftBracket::Undrafted);
        assert_eq!(DraftBracket::of(None, Some(3)), DraftBracket::Undrafted);
    }

    #[test]
    fn the_table_pools_each_bracket_with_enough_rookies_and_shrinks_by_minutes() {
        let mut samples = Vec::new();
        for i in 0..10 {
            samples.push(RookieSample {
                bracket: DraftBracket::Top5,
                worth_elo: 10.0 + f64::from(i) * 2.0, // 10 to 28: mean 19, sd 6.06
            });
        }
        for _ in 0..9 {
            samples.push(RookieSample {
                bracket: DraftBracket::Lottery,
                worth_elo: 5.0,
            });
        }
        let t = DraftTable::fit(&samples);
        assert_eq!(t.players, 19);
        assert_eq!(t.priors.len(), 1, "nine lottery picks are too few");
        let p = t.prior(DraftBracket::Top5).unwrap();
        assert_eq!(p.players, 10);
        assert!((p.mean_elo - 19.0).abs() < 1e-12);
        assert!((p.sd_elo - (330.0f64 / 9.0).sqrt()).abs() < 1e-9);
        assert!((p.se_elo - (330.0f64 / 90.0).sqrt()).abs() < 1e-9);
        assert!(t.prior(DraftBracket::Lottery).is_none());
        // The first pick has no prior of its own: picks 2 to 5 stand in. The lottery, with
        // too few, has nobody wider with a prior; the second round never has a stand-in.
        assert_eq!(
            t.prior_for(DraftBracket::FirstPick).unwrap().bracket,
            DraftBracket::Top5
        );
        assert!(t.prior_for(DraftBracket::Lottery).is_none());
        assert!(t.prior_for(DraftBracket::SecondRound).is_none());
        // No minutes: the prior and the error of its mean. At the reference minutes:
        // halfway. Many minutes: his own worth, with little of the error left.
        assert_eq!(shrink(p, None), (p.mean_elo, p.se_elo));
        assert_eq!(shrink(p, Some((40.0, 0.0))), (p.mean_elo, p.se_elo));
        let (elo, se) = shrink(p, Some((40.0, PRIOR_MINUTES)));
        assert!((elo - 29.5).abs() < 1e-12);
        assert!((se - p.se_elo / 2.0).abs() < 1e-12);
        let (elo, se) = shrink(p, Some((40.0, 9.0 * PRIOR_MINUTES)));
        assert!((elo - (0.9 * 40.0 + 0.1 * 19.0)).abs() < 1e-12);
        assert!((se - 0.1 * p.se_elo).abs() < 1e-12);
    }
}
