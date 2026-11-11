//! Career worth: what one player was worth to his teams over his whole career, in Elo, with
//! the error of that number and the tier it puts him in.
//!
//! Two readings of a career go into it. The first is the gap: inside a stint (one player
//! with one team in one season, as a regular; the store applies the [`CellRule`]) the team is
//! the same team with and without the player, so the mean adjusted margin of the games he
//! played less the mean of those he missed is his. A margin is adjusted for the opponent's
//! pre-game Elo and for the home court; the player's own team's rating is not used, because
//! it adapts while he is out. The career gap pools the stints' gaps, each weighted by its
//! effective number of games, at [`ELO_PER_POINT`]. An absence is a team game between his
//! first and his last game of the stint, the same absence the season's player worth reads.
//! The store has no rosters for past seasons, so the rule cannot see a season a player sat
//! out in whole; a [`DeclaredAbsence`] says by hand what the store cannot, and
//! [`DECLARED_ABSENCES`] is the whole list.
//!
//! The gap alone is too noisy to rank by, and it says nothing of a player who never missed a
//! game. The second reading is the career box worth: the box-score worth of his seasons,
//! weighted by their minutes. A regression of the gaps on the box worths, over the players
//! with both, turns a box worth into a prior for the gap with its own error, and a player's
//! career worth is the two weighted by their precision. A player without an absence is
//! ranked on the prior alone.
//!
//! The worths are still uncertain, so the ranking does not sort them. The ranking is drawn
//! many times from the worths and their errors, and a player's tier is the smallest group he
//! is more likely than not to belong to. Everything here is a pure function of the cells,
//! the box seasons, the settings and the random stream (REAS-ARCH-005).

use std::collections::BTreeMap;

use rand::Rng;
use serde::Serialize;

use super::elo::DEFAULT_HOME_ADVANTAGE;
use super::ids::{PlayerId, SeasonYear, TeamId};
use super::worth::{self, ELO_PER_POINT};

pub const CAREER_MODEL_VERSION: &str = "career-gap-box-2.1";

/// One season that carries more than this share of a player's gap is named beside him.
pub const DOMINANT_SEASON_SHARE: f64 = 1.0 / 3.0;
/// The sizes of the groups the chances are counted for, the smallest first.
pub const GROUPS: [usize; 4] = [10, 25, 50, 100];
/// The calibration needs this many players with a gap and a box worth: under it a slope and
/// a residual spread are not worth reading, and no player is ranked.
pub const MIN_CALIBRATION_PLAYERS: usize = 30;
/// The least error of a prior, in Elo. A box score cannot say a worth exactly; without the
/// floor a residual spread that comes out at zero would let the prior silence every gap.
pub const MIN_PRIOR_ERROR: f64 = 5.0;

/// Team games a player sat out that the stint rule cannot see, declared by hand: every game
/// his team played in `season` is read as inside his stint of `stint_season` with that
/// team. The games he missed become absences of that stint, against the games he played in
/// it. A declaration is a judgement that he was that team's player the whole time and that
/// the team around him was near enough the same; the rule makes neither for any other
/// player, so a declared player is measured with more evidence than his peers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct DeclaredAbsence {
    /// The player's key at the player source: a store keeps it across loads, and the
    /// database key is its own.
    pub source: &'static str,
    pub source_id: i64,
    pub team_code: &'static str,
    /// The stint that receives the games; without it the declaration adds nothing.
    pub stint_season: SeasonYear,
    /// The season whose team games are all read as inside that stint.
    pub season: SeasonYear,
    /// What the declaration is, for the page.
    pub note: &'static str,
}

const JORDAN_RETIREMENT: &str =
    "His retirement from October 1993 to March 1995 counts as games missed";

/// Every declared absence: Michael Jordan's first retirement, and no other. He left the
/// Bulls on 6 October 1993 and played again on 19 March 1995. The 82 games of 1993-94 are
/// read against the games he played in 1992-93, the season before, and the 65 games of
/// 1994-95 before his return against the 17 he played after it. ADR 0020 has the reason
/// and the runs.
pub const DECLARED_ABSENCES: &[DeclaredAbsence] = &[
    DeclaredAbsence {
        source: "balldontlie",
        source_id: 2931,
        team_code: "CHI",
        stint_season: SeasonYear::of(1993),
        season: SeasonYear::of(1994),
        note: JORDAN_RETIREMENT,
    },
    DeclaredAbsence {
        source: "balldontlie",
        source_id: 2931,
        team_code: "CHI",
        stint_season: SeasonYear::of(1995),
        season: SeasonYear::of(1995),
        note: JORDAN_RETIREMENT,
    },
];

/// Which games make a stint, and how a margin is adjusted. The store applies it.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct CellRule {
    /// The first season read. Box scores cover most games from 1976-77.
    pub first_season: SeasonYear,
    /// A stint needs this many games played for the team in the season.
    pub min_games: u32,
    /// A stint needs this many minutes a game, so that a benched player's games out are not
    /// read as absences.
    pub min_minutes: f64,
    /// The home side's advantage taken out of a margin, in Elo.
    pub home_advantage: f64,
    pub elo_per_point: f64,
    /// The absences declared by hand, read on top of the stints.
    pub declared: &'static [DeclaredAbsence],
}

impl Default for CellRule {
    fn default() -> Self {
        Self {
            first_season: SeasonYear::try_new(1977)
                .expect("1977 is inside the supported range of seasons"),
            min_games: 5,
            min_minutes: 24.0,
            home_advantage: DEFAULT_HOME_ADVANTAGE,
            elo_per_point: ELO_PER_POINT,
            declared: DECLARED_ABSENCES,
        }
    }
}

/// The games of one stint that the player played, or those he missed: their count and the
/// sum and sum of squares of the team's adjusted margins, in points.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Cell {
    pub player_id: PlayerId,
    pub season: SeasonYear,
    pub team_id: TeamId,
    pub played: bool,
    pub games: u32,
    pub sum: f64,
    pub sum_sq: f64,
}

/// One player's box-score worth in one season with one team, with the minutes behind it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BoxSeason {
    pub player_id: PlayerId,
    pub season: SeasonYear,
    pub minutes: f64,
    pub worth_elo: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Settings {
    /// A player is ranked with this many games played as a regular, over every stint.
    pub min_games: u32,
    /// How many times the ranking is drawn.
    pub draws: u32,
    pub seed: u64,
    /// How many players the list holds.
    pub listed: usize,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            min_games: 400,
            draws: 20_000,
            seed: 1977,
            listed: 100,
        }
    }
}

/// The smallest group a player is more likely than not to belong to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(into = "u8")]
pub enum Tier {
    /// More likely than not one of the best 10.
    One,
    /// More likely than not one of the best 25.
    Two,
    /// More likely than not one of the best 50.
    Three,
    /// More likely than not one of the best 100.
    Four,
    /// Less than an even chance of the best 100.
    Five,
}

impl Tier {
    /// The tier for the chances of the [`GROUPS`], the smallest group first.
    pub fn of(chances: [f64; 4]) -> Tier {
        const TIERS: [Tier; 4] = [Tier::One, Tier::Two, Tier::Three, Tier::Four];
        chances
            .iter()
            .zip(TIERS)
            .find(|(chance, _)| **chance >= 0.5)
            .map_or(Tier::Five, |(_, tier)| tier)
    }

    pub const fn number(self) -> u8 {
        match self {
            Tier::One => 1,
            Tier::Two => 2,
            Tier::Three => 3,
            Tier::Four => 4,
            Tier::Five => 5,
        }
    }
}

impl From<Tier> for u8 {
    fn from(tier: Tier) -> u8 {
        tier.number()
    }
}

/// What the games with and without a player say of him, over the stints that have both.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Gap {
    /// Elo: the pooled gap between his teams' adjusted margins with him and without him.
    pub worth: f64,
    /// The standard error of `worth`, in Elo.
    pub error: f64,
    /// Games played and team games missed in the stints that have both.
    pub played: u32,
    pub missed: u32,
    /// The season that carries more than [`DOMINANT_SEASON_SHARE`] of the evidence, if one
    /// does.
    pub dominant_season: Option<SeasonYear>,
}

/// One player's career as a regular, from his stints, before any ranking.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Career {
    pub player_id: PlayerId,
    /// Games played as a regular, over every stint.
    pub career_games: u32,
    pub first_season: SeasonYear,
    pub last_season: SeasonYear,
    /// None when no stint has games on both sides, or too few to give an error: his teams
    /// played no game without him between his first game and his last.
    pub gap: Option<Gap>,
}

/// How a career box worth reads as a gap: the line fitted over the players with both, and
/// how far a player's true gap lies from the line.
///
/// The line has an intercept, unlike the season's box calibration. The two numbers do not
/// share a zero: the box worth is against a replacement level set by a formula, and the gap
/// is against the players who took the minutes on that team in those games.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Calibration {
    /// Players with a gap and a box worth: the fit is over these.
    pub players: usize,
    pub intercept: f64,
    /// Elo of gap for one Elo of box worth.
    pub slope: f64,
    pub slope_error: f64,
    /// The weighted mean box worth of the players, and the error of the line there.
    pub mean_box: f64,
    pub mean_error: f64,
    /// The spread of the true gaps around the line, in Elo: what the box score does not see.
    pub spread: f64,
}

impl Calibration {
    /// The prior for the gap of a player with this career box worth: the line's value and
    /// its error, from the spread around the line and the error of the line itself.
    pub fn prior(&self, box_worth: f64) -> (f64, f64) {
        let off_centre = self.slope_error * (box_worth - self.mean_box);
        let variance =
            self.spread * self.spread + self.mean_error * self.mean_error + off_centre * off_centre;
        (
            self.intercept + self.slope * box_worth,
            variance.sqrt().max(MIN_PRIOR_ERROR),
        )
    }
}

/// One ranked player.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Ranked {
    /// The place among the ranked players: by tier, then by expected rank.
    pub rank: usize,
    pub tier: Tier,
    #[serde(flatten)]
    pub career: Career,
    /// The box-score worth of his seasons, weighted by their minutes, in Elo.
    pub box_worth: f64,
    /// What the calibration expects his gap to be from the box worth, and its error.
    pub prior: f64,
    pub prior_error: f64,
    /// The career worth: the prior and the gap weighted by their precision, and its error.
    pub worth: f64,
    pub error: f64,
    /// The mean place over the draws, among every ranked player.
    pub expected_rank: f64,
    /// The share of the draws that put him in each of the [`GROUPS`].
    pub top10: f64,
    pub top25: f64,
    pub top50: f64,
    pub top100: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Ranking {
    pub model: &'static str,
    pub settings: Settings,
    /// Players with a stint.
    pub measured: usize,
    /// The population the ranks and the chances are among: the players with the games the
    /// settings ask and a career box worth.
    pub ranked: usize,
    /// Players with fewer games as a regular than the settings ask: not reported.
    pub short_careers: usize,
    /// Players with the games and no box worth in any season: not ranked.
    pub without_box: usize,
    /// Ranked players without a gap: ranked on the prior alone.
    pub without_gap: usize,
    /// None when too few players have a gap and a box worth; then no player is ranked.
    pub calibration: Option<Calibration>,
    /// The spread of the ranked players' worths, the root mean square of the errors of
    /// those worths, and that of the errors of their gaps, in Elo.
    pub spread: f64,
    pub typical_error: f64,
    pub typical_gap_error: f64,
    /// How many of the list's players the draws expect to belong to a group of the list's
    /// size: the sum of their chances.
    pub expected_in_list: f64,
    /// The size of each tier among the list's players, tier one first.
    pub tier_sizes: [usize; 5],
    /// Every ranked player in rank order; the list is the first `settings.listed`.
    pub players: Vec<Ranked>,
    pub half_check: Option<HalfCheck>,
}

/// A check that a worth says something of the games it has not seen. A player's odd seasons
/// and his even seasons share no game. Each estimate is built on one half (the gap, the box
/// worth and the calibration of that half alone) and scored against the gap of the other
/// half, in both directions.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct HalfCheck {
    /// Player halves scored: a player with a gap in both halves counts twice.
    pub pairs: usize,
    /// The gap alone, shrunk toward the mean of the gaps by its error: the first version of
    /// the ranking.
    pub gap: Score,
    /// The prior from the box worth alone.
    pub prior: Score,
    /// The career worth as ranked: the prior and the gap together.
    pub blend: Score,
}

/// How one estimate did against the other half's gaps, each weighted by its precision.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Score {
    /// The correlation between the estimates and the other half's gaps.
    pub correlation: f64,
    /// The root mean square miss beyond the other half's own errors, in Elo; zero for an
    /// estimate that misses by no more than those errors.
    pub excess_error: f64,
    /// The spread of the misses over the errors they were given (the other half's and the
    /// estimate's own). One when the errors are honest; above one when they are too small.
    pub z_spread: f64,
}

#[derive(Debug, Clone, Copy, Default)]
struct Moments {
    games: u32,
    sum: f64,
    sum_sq: f64,
}

impl Moments {
    fn mean(self) -> f64 {
        self.sum / f64::from(self.games)
    }
    /// The sum of squared distances from the mean.
    fn scatter(self) -> f64 {
        (self.sum_sq - self.sum * self.sum / f64::from(self.games)).max(0.0)
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct Stint {
    with: Moments,
    without: Moments,
}

/// Each player's career from the cells, in player order: every player with a stint, with
/// the gap when his stints give one.
pub fn measure(cells: &[Cell], elo_per_point: f64) -> Vec<Career> {
    let mut stints: BTreeMap<(PlayerId, SeasonYear, TeamId), Stint> = BTreeMap::new();
    for cell in cells {
        let stint = stints
            .entry((cell.player_id, cell.season, cell.team_id))
            .or_default();
        let side = if cell.played {
            &mut stint.with
        } else {
            &mut stint.without
        };
        side.games += cell.games;
        side.sum += cell.sum;
        side.sum_sq += cell.sum_sq;
    }
    let mut players: BTreeMap<PlayerId, Vec<(SeasonYear, Stint)>> = BTreeMap::new();
    for ((player_id, season, _), stint) in stints {
        players.entry(player_id).or_default().push((season, stint));
    }
    players
        .into_iter()
        .filter_map(|(player_id, stints)| {
            Some(Career {
                player_id,
                career_games: stints.iter().map(|(_, s)| s.with.games).sum(),
                first_season: stints.iter().map(|(season, _)| *season).min()?,
                last_season: stints.iter().map(|(season, _)| *season).max()?,
                gap: gap(&stints, elo_per_point),
            })
        })
        .collect()
}

fn gap(stints: &[(SeasonYear, Stint)], elo_per_point: f64) -> Option<Gap> {
    let (mut weight, mut weighted_gap, mut scatter) = (0.0, 0.0, 0.0);
    let (mut played, mut missed, mut freedom) = (0u32, 0u32, 0u32);
    let mut by_season: BTreeMap<SeasonYear, f64> = BTreeMap::new();
    for (season, stint) in stints {
        let (with, without) = (stint.with, stint.without);
        if with.games == 0 || without.games == 0 {
            continue;
        }
        let (n_with, n_without) = (f64::from(with.games), f64::from(without.games));
        let h = n_with * n_without / (n_with + n_without);
        weight += h;
        weighted_gap += h * (with.mean() - without.mean());
        scatter += with.scatter() + without.scatter();
        freedom += with.games + without.games - 2;
        played += with.games;
        missed += without.games;
        *by_season.entry(*season).or_insert(0.0) += h;
    }
    if weight <= 0.0 || freedom == 0 {
        return None;
    }
    let variance = scatter / f64::from(freedom);
    let dominant_season = by_season
        .iter()
        .find(|(_, h)| **h > DOMINANT_SEASON_SHARE * weight)
        .filter(|_| by_season.len() > 1)
        .map(|(season, _)| *season);
    Some(Gap {
        worth: elo_per_point * weighted_gap / weight,
        error: elo_per_point * (variance / weight).sqrt(),
        played,
        missed,
        dominant_season,
    })
}

/// Each player's career box worth: his seasons' box worths weighted by their minutes. A
/// player without minutes is absent.
pub fn career_box(seasons: &[BoxSeason]) -> BTreeMap<PlayerId, f64> {
    let mut by_player: BTreeMap<PlayerId, Vec<(f64, f64)>> = BTreeMap::new();
    for s in seasons {
        by_player
            .entry(s.player_id)
            .or_default()
            .push((s.worth_elo, s.minutes));
    }
    by_player
        .into_iter()
        .filter_map(|(player_id, seasons)| Some((player_id, worth::pool_box(&seasons)?)))
        .collect()
}

/// The line of the gaps on the box worths, over `(box worth, gap, error of the gap)`. Each
/// player weighs by the inverse of his gap's variance plus the spread around the line, and
/// the spread is the weighted mean of the squared residuals beyond the errors, so the two
/// are found together by iteration. A gap without a positive error is left out. None under
/// [`MIN_CALIBRATION_PLAYERS`], or when the box worths do not differ.
pub fn calibrate(pairs: &[(f64, f64, f64)]) -> Option<Calibration> {
    let pairs: Vec<(f64, f64, f64)> = pairs
        .iter()
        .copied()
        .filter(|(x, y, e)| x.is_finite() && y.is_finite() && e.is_finite() && *e > 0.0)
        .collect();
    if pairs.len() < MIN_CALIBRATION_PLAYERS {
        return None;
    }
    let mut spread = 0.0f64;
    let mut fit = None;
    // The spread settles in a few rounds on the stored data; the bound is for data that
    // makes it wander.
    for _ in 0..200 {
        let weights: Vec<f64> = pairs
            .iter()
            .map(|(_, _, e)| 1.0 / (e * e + spread * spread))
            .collect();
        let total: f64 = weights.iter().sum();
        let sum = |f: &dyn Fn(&(f64, f64, f64)) -> f64| -> f64 {
            weights.iter().zip(&pairs).map(|(w, p)| w * f(p)).sum()
        };
        let mean_box = sum(&|p| p.0) / total;
        let mean_gap = sum(&|p| p.1) / total;
        let sxx = sum(&|p| (p.0 - mean_box).powi(2));
        if sxx <= 0.0 || !sxx.is_finite() {
            return None;
        }
        let slope = sum(&|p| (p.0 - mean_box) * (p.1 - mean_gap)) / sxx;
        let intercept = mean_gap - slope * mean_box;
        let beyond = sum(&|p| (p.1 - intercept - slope * p.0).powi(2) - p.2 * p.2) / total;
        let next = beyond.max(0.0).sqrt();
        let settled = (next - spread).abs() < 1e-6;
        fit = Some(Calibration {
            players: pairs.len(),
            intercept,
            slope,
            slope_error: (1.0 / sxx).sqrt(),
            mean_box,
            mean_error: (1.0 / total).sqrt(),
            spread: next,
        });
        spread = next;
        if settled {
            break;
        }
    }
    fit
}

/// What the estimate reads of one player: his career box worth and, when he has one, his
/// gap with its error.
#[derive(Debug, Clone, Copy)]
struct Evidence {
    box_worth: f64,
    gap: Option<(f64, f64)>,
}

/// A player's prior and his worth, each with its error.
#[derive(Debug, Clone, Copy)]
struct Estimate {
    prior: (f64, f64),
    worth: (f64, f64),
}

/// The calibration over the players with a gap, and every player's prior and worth under
/// it. The ranking and the half-career check both come through here, so the check scores
/// the estimate the page shows.
fn estimate(players: &[Evidence]) -> Option<(Calibration, Vec<Estimate>)> {
    let pairs: Vec<(f64, f64, f64)> = players
        .iter()
        .filter_map(|p| p.gap.map(|(gap, error)| (p.box_worth, gap, error)))
        .collect();
    let calibration = calibrate(&pairs)?;
    let estimates = players
        .iter()
        .map(|p| {
            let prior = calibration.prior(p.box_worth);
            Estimate {
                prior,
                worth: p.gap.map_or(prior, |gap| worth::combine(prior, gap)),
            }
        })
        .collect();
    Some((calibration, estimates))
}

/// One standard normal value (Box and Muller).
fn standard_normal<R: Rng>(rng: &mut R) -> f64 {
    let u: f64 = rng.gen_range(f64::MIN_POSITIVE..1.0);
    let v: f64 = rng.gen();
    (-2.0 * u.ln()).sqrt() * (std::f64::consts::TAU * v).cos()
}

fn root_mean_square(values: impl Iterator<Item = f64>) -> f64 {
    let (n, sum) = values.fold((0usize, 0.0), |(n, sum), v| (n + 1, sum + v * v));
    if n == 0 {
        0.0
    } else {
        (sum / n as f64).sqrt()
    }
}

/// Rank the careers: the players with the games the settings ask and a career box worth
/// get a prior and a worth, are drawn, put in tiers and ordered; the list is the first
/// `settings.listed` of them. `half_check` is carried into the result as given.
pub fn rank<R: Rng>(
    careers: Vec<Career>,
    box_worths: &BTreeMap<PlayerId, f64>,
    settings: Settings,
    half_check: Option<HalfCheck>,
    rng: &mut R,
) -> Ranking {
    let measured = careers.len();
    let (long, short): (Vec<Career>, Vec<Career>) = careers
        .into_iter()
        .partition(|c| c.career_games >= settings.min_games);
    let long_careers = long.len();
    let with_box: Vec<(Career, f64)> = long
        .into_iter()
        .filter_map(|c| Some((*box_worths.get(&c.player_id)?, c)))
        .map(|(box_worth, c)| (c, box_worth))
        .collect();
    let without_box = long_careers - with_box.len();
    let evidence: Vec<Evidence> = with_box
        .iter()
        .map(|(c, box_worth)| Evidence {
            box_worth: *box_worth,
            gap: c.gap.map(|g| (g.worth, g.error)),
        })
        .collect();
    let Some((calibration, estimates)) = estimate(&evidence) else {
        return Ranking {
            model: CAREER_MODEL_VERSION,
            settings,
            measured,
            ranked: 0,
            short_careers: short.len(),
            without_box,
            without_gap: 0,
            calibration: None,
            spread: 0.0,
            typical_error: 0.0,
            typical_gap_error: 0.0,
            expected_in_list: 0.0,
            tier_sizes: [0; 5],
            players: Vec::new(),
            half_check,
        };
    };

    let n = with_box.len();
    let mut rank_sum = vec![0.0f64; n];
    let mut in_group = vec![[0u32; 4]; n];
    let mut order: Vec<usize> = (0..n).collect();
    let mut drawn = vec![0.0f64; n];
    for _ in 0..settings.draws {
        for (value, e) in drawn.iter_mut().zip(&estimates) {
            *value = e.worth.0 + e.worth.1 * standard_normal(rng);
        }
        // The stable sort keeps the earlier order on a tie, so the result does not depend
        // on the sort's internals.
        order.sort_by(|a, b| drawn[*b].total_cmp(&drawn[*a]).then(a.cmp(b)));
        for (place, player) in order.iter().enumerate() {
            rank_sum[*player] += (place + 1) as f64;
            for (count, size) in in_group[*player].iter_mut().zip(GROUPS) {
                if place < size {
                    *count += 1;
                }
            }
        }
    }
    let draws = f64::from(settings.draws.max(1));
    let mean_worth = estimates.iter().map(|e| e.worth.0).sum::<f64>() / n as f64;
    let spread = root_mean_square(estimates.iter().map(|e| e.worth.0 - mean_worth));
    let typical_error = root_mean_square(estimates.iter().map(|e| e.worth.1));
    let typical_gap_error = root_mean_square(evidence.iter().filter_map(|p| Some(p.gap?.1)));
    let without_gap = evidence.iter().filter(|p| p.gap.is_none()).count();
    let mut rows: Vec<Ranked> = with_box
        .into_iter()
        .zip(&estimates)
        .enumerate()
        .map(|(i, ((career, box_worth), e))| {
            let chances = in_group[i].map(|count| f64::from(count) / draws);
            Ranked {
                rank: 0,
                tier: Tier::of(chances),
                career,
                box_worth,
                prior: e.prior.0,
                prior_error: e.prior.1,
                worth: e.worth.0,
                error: e.worth.1,
                expected_rank: rank_sum[i] / draws,
                top10: chances[0],
                top25: chances[1],
                top50: chances[2],
                top100: chances[3],
            }
        })
        .collect();
    rows.sort_by(|a, b| {
        a.tier
            .cmp(&b.tier)
            .then(a.expected_rank.total_cmp(&b.expected_rank))
            .then(b.worth.total_cmp(&a.worth))
            .then(a.career.player_id.cmp(&b.career.player_id))
    });
    for (i, row) in rows.iter_mut().enumerate() {
        row.rank = i + 1;
    }
    let list = &rows[..rows.len().min(settings.listed)];
    let list_group = GROUPS
        .iter()
        .position(|size| *size >= settings.listed)
        .unwrap_or(GROUPS.len() - 1);
    let expected_in_list = list
        .iter()
        .map(|r| [r.top10, r.top25, r.top50, r.top100][list_group])
        .sum();
    let mut tier_sizes = [0usize; 5];
    for row in list {
        tier_sizes[usize::from(row.tier.number()) - 1] += 1;
    }
    Ranking {
        model: CAREER_MODEL_VERSION,
        settings,
        measured,
        ranked: n,
        short_careers: short.len(),
        without_box,
        without_gap,
        calibration: Some(calibration),
        spread,
        typical_error,
        typical_gap_error,
        expected_in_list,
        tier_sizes,
        players: rows,
        half_check,
    }
}

/// The gap of the other half, with the three estimates built without it: the gap alone,
/// the prior alone, the two together.
#[derive(Debug, Clone, Copy)]
struct Scored {
    target: f64,
    target_error: f64,
    estimates: [(f64, f64); 3],
}

fn score(pairs: &[Scored], which: usize) -> Score {
    let weight = |p: &Scored| 1.0 / (p.target_error * p.target_error);
    let total: f64 = pairs.iter().map(weight).sum();
    let mean = |f: &dyn Fn(&Scored) -> f64| -> f64 {
        pairs.iter().map(|p| weight(p) * f(p)).sum::<f64>() / total
    };
    let (mean_estimate, mean_target) = (mean(&|p| p.estimates[which].0), mean(&|p| p.target));
    let ee = mean(&|p| (p.estimates[which].0 - mean_estimate).powi(2));
    let tt = mean(&|p| (p.target - mean_target).powi(2));
    let et = mean(&|p| (p.estimates[which].0 - mean_estimate) * (p.target - mean_target));
    let correlation = if ee > 0.0 && tt > 0.0 {
        et / (ee * tt).sqrt()
    } else {
        0.0
    };
    // Each squared miss holds the other half's own error once: with these weights that is
    // one for every pair.
    let miss = mean(&|p| (p.target - p.estimates[which].0).powi(2));
    let noise = pairs.len() as f64 / total;
    let z: Vec<f64> = pairs
        .iter()
        .map(|p| {
            let (estimate, error) = p.estimates[which];
            (p.target - estimate) / (p.target_error * p.target_error + error * error).sqrt()
        })
        .collect();
    let mean_z = z.iter().sum::<f64>() / z.len() as f64;
    Score {
        correlation,
        excess_error: (miss - noise).max(0.0).sqrt(),
        z_spread: root_mean_square(z.iter().map(|v| v - mean_z)),
    }
}

/// The mean of the gaps, and the variance of the true gaps behind them: the variance of the
/// gaps less their mean squared error, never below zero.
fn common(gaps: &[(f64, f64)]) -> (f64, f64) {
    if gaps.is_empty() {
        return (0.0, 0.0);
    }
    let n = gaps.len() as f64;
    let mean = gaps.iter().map(|g| g.0).sum::<f64>() / n;
    if gaps.len() < 2 {
        return (mean, 0.0);
    }
    let variance = gaps.iter().map(|g| (g.0 - mean).powi(2)).sum::<f64>() / (n - 1.0);
    let noise = gaps.iter().map(|g| g.1 * g.1).sum::<f64>() / n;
    (mean, (variance - noise).max(0.0))
}

/// The half-career check over the players with `min_games` games as a regular on all their
/// seasons. In each direction a player is scored when the building half gives him a box
/// worth and the other half a gap. None when a half has no calibration, or with fewer than
/// three scored halves.
pub fn half_check(
    cells: &[Cell],
    boxes: &[BoxSeason],
    min_games: u32,
    elo_per_point: f64,
) -> Option<HalfCheck> {
    let long: Vec<PlayerId> = measure(cells, elo_per_point)
        .into_iter()
        .filter(|c| c.career_games >= min_games)
        .map(|c| c.player_id)
        .collect();
    let odd = |season: SeasonYear| season.end_year() % 2 != 0;
    let gaps = |build_odd: bool| -> BTreeMap<PlayerId, (f64, f64)> {
        let half: Vec<Cell> = cells
            .iter()
            .filter(|c| odd(c.season) == build_odd)
            .copied()
            .collect();
        measure(&half, elo_per_point)
            .into_iter()
            .filter_map(|c| Some((c.player_id, c.gap.map(|g| (g.worth, g.error))?)))
            .collect()
    };
    let (odd_gaps, even_gaps) = (gaps(true), gaps(false));
    let mut pairs: Vec<Scored> = Vec::new();
    for build_odd in [true, false] {
        let (built, other) = if build_odd {
            (&odd_gaps, &even_gaps)
        } else {
            (&even_gaps, &odd_gaps)
        };
        let half: Vec<BoxSeason> = boxes
            .iter()
            .filter(|b| odd(b.season) == build_odd)
            .copied()
            .collect();
        let box_worths = career_box(&half);
        let players: Vec<(Evidence, (f64, f64))> = long
            .iter()
            .filter_map(|id| {
                let target = *other.get(id)?;
                let evidence = Evidence {
                    box_worth: *box_worths.get(id)?,
                    gap: built.get(id).copied(),
                };
                (target.1 > 0.0).then_some((evidence, target))
            })
            .collect();
        let evidence: Vec<Evidence> = players.iter().map(|(e, _)| *e).collect();
        let (_, estimates) = estimate(&evidence)?;
        let built_gaps: Vec<(f64, f64)> = evidence.iter().filter_map(|e| e.gap).collect();
        let (mean, spread_sq) = common(&built_gaps);
        for ((evidence, target), e) in players.iter().zip(&estimates) {
            let alone = match evidence.gap {
                Some((gap, error)) => {
                    let noise = error * error;
                    let keep = if spread_sq + noise > 0.0 {
                        spread_sq / (spread_sq + noise)
                    } else {
                        0.0
                    };
                    (mean + keep * (gap - mean), (keep * noise).sqrt())
                }
                None => (mean, spread_sq.sqrt()),
            };
            pairs.push(Scored {
                target: target.0,
                target_error: target.1,
                estimates: [alone, e.prior, e.worth],
            });
        }
    }
    if pairs.len() < 3 {
        return None;
    }
    Some(HalfCheck {
        pairs: pairs.len(),
        gap: score(&pairs, 0),
        prior: score(&pairs, 1),
        blend: score(&pairs, 2),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;
    use rand_chacha::ChaCha8Rng;

    fn season(year: i32) -> SeasonYear {
        SeasonYear::try_new(year).unwrap()
    }

    /// The list of declared absences is one player's, and each row can be read: the stint
    /// is in the season of the games or before it, inside the seasons the rule reads, and no
    /// season of a team is declared twice.
    #[test]
    fn the_declared_absences_are_michael_jordans_retirement_and_no_other() {
        let rule = CellRule::default();
        assert_eq!(rule.declared, DECLARED_ABSENCES);
        assert_eq!(DECLARED_ABSENCES.len(), 2);
        for (i, d) in DECLARED_ABSENCES.iter().enumerate() {
            assert_eq!(
                (d.source, d.source_id, d.team_code),
                ("balldontlie", 2931, "CHI")
            );
            assert!(rule.first_season <= d.stint_season && d.stint_season <= d.season);
            assert!(d.note.contains("1993") && d.note.contains("1995"));
            assert!(DECLARED_ABSENCES[..i].iter().all(|e| e.season != d.season));
        }
        assert_eq!(
            DECLARED_ABSENCES
                .iter()
                .map(|d| d.season)
                .collect::<Vec<_>>(),
            [season(1994), season(1995)]
        );
    }

    /// Both cells of a stint from the adjusted margins of the games played and missed.
    fn stint(player: i32, year: i32, team: i32, with: &[f64], without: &[f64]) -> Vec<Cell> {
        [(true, with), (false, without)]
            .into_iter()
            .filter(|(_, margins)| !margins.is_empty())
            .map(|(played, margins)| Cell {
                player_id: PlayerId::new(player),
                season: season(year),
                team_id: TeamId::new(team),
                played,
                games: margins.len() as u32,
                sum: margins.iter().sum(),
                sum_sq: margins.iter().map(|m| m * m).sum(),
            })
            .collect()
    }

    fn career(player: i32, gap: Option<(f64, f64)>) -> Career {
        Career {
            player_id: PlayerId::new(player),
            career_games: 500,
            first_season: season(2000),
            last_season: season(2010),
            gap: gap.map(|(worth, error)| Gap {
                worth,
                error,
                played: 400,
                missed: 100,
                dominant_season: None,
            }),
        }
    }

    fn settings() -> Settings {
        Settings {
            min_games: 400,
            draws: 2_000,
            seed: 7,
            listed: 100,
        }
    }

    fn rng(seed: u64) -> ChaCha8Rng {
        ChaCha8Rng::seed_from_u64(seed)
    }

    /// Thirty players, two at each of the box worths 0, 10, ..., 140: one with a gap 30
    /// above the line 10 + 0.5 × box and one 30 below it, every gap with an error of 18.
    /// The residuals balance at every box worth, so the fit is the line itself.
    fn balanced() -> (Vec<Career>, BTreeMap<PlayerId, f64>) {
        let mut careers = Vec::new();
        let mut boxes = BTreeMap::new();
        for i in 0..30 {
            let box_worth = f64::from(i / 2) * 10.0;
            let off = if i % 2 == 0 { 30.0 } else { -30.0 };
            careers.push(career(i, Some((10.0 + 0.5 * box_worth + off, 18.0))));
            boxes.insert(PlayerId::new(i), box_worth);
        }
        (careers, boxes)
    }

    fn row(r: &Ranking, id: i32) -> &Ranked {
        r.players
            .iter()
            .find(|row| row.career.player_id == PlayerId::new(id))
            .unwrap()
    }

    #[test]
    fn one_stint_gives_the_gap_and_its_error() {
        // With: 6, 8, 10 (mean 8). Without: 1, 3 (mean 2). Gap 6 points.
        let cells = stint(1, 2010, 1, &[6.0, 8.0, 10.0], &[1.0, 3.0]);
        let m = measure(&cells, 28.0);
        assert_eq!(m.len(), 1);
        let gap = m[0].gap.unwrap();
        assert!((gap.worth - 168.0).abs() < 1e-9);
        // Scatter 8 + 2 over 3 degrees of freedom; weight 3 × 2 / 5 = 1.2.
        let error = 28.0 * ((10.0 / 3.0) / 1.2f64).sqrt();
        assert!((gap.error - error).abs() < 1e-9);
        assert_eq!((gap.played, gap.missed, m[0].career_games), (3, 2, 3));
        assert_eq!(gap.dominant_season, None);
    }

    #[test]
    fn stints_pool_by_their_effective_games() {
        // Stint one: gap 10, weight 2 × 2 / 4 = 1. Stint two: gap 0, weight 4 × 4 / 8 = 2.
        let mut cells = stint(1, 2010, 1, &[11.0, 9.0], &[1.0, -1.0]);
        cells.extend(stint(
            1,
            2011,
            1,
            &[1.0, -1.0, 1.0, -1.0],
            &[1.0, -1.0, 1.0, -1.0],
        ));
        let m = measure(&cells, 1.0);
        let gap = m[0].gap.unwrap();
        assert!((gap.worth - 10.0 / 3.0).abs() < 1e-9);
        // 2011 carries two thirds of the weight.
        assert_eq!(gap.dominant_season, Some(season(2011)));
        assert_eq!(
            (m[0].first_season, m[0].last_season),
            (season(2010), season(2011))
        );
    }

    #[test]
    fn a_stint_without_absences_adds_games_and_no_evidence() {
        let mut cells = stint(1, 2010, 1, &[6.0, 8.0, 10.0], &[1.0, 3.0]);
        let alone = measure(&cells, 28.0)[0].gap.unwrap();
        cells.extend(stint(1, 2011, 1, &[30.0, 40.0, 50.0, 60.0], &[]));
        let m = measure(&cells, 28.0);
        let gap = m[0].gap.unwrap();
        assert_eq!((gap.worth, gap.error), (alone.worth, alone.error));
        assert_eq!((m[0].career_games, gap.played, gap.missed), (7, 3, 2));
        assert_eq!(m[0].last_season, season(2011));
    }

    #[test]
    fn a_player_with_no_two_sided_stint_has_a_career_and_no_gap() {
        let mut cells = stint(1, 2010, 1, &[6.0, 8.0], &[]);
        // One game on each side leaves no degree of freedom for an error.
        cells.extend(stint(2, 2010, 1, &[6.0], &[1.0]));
        let m = measure(&cells, 28.0);
        assert_eq!(
            m.iter().map(|c| c.career_games).collect::<Vec<_>>(),
            vec![2, 1]
        );
        assert!(m.iter().all(|c| c.gap.is_none()));
    }

    #[test]
    fn the_career_box_worth_weights_each_season_by_its_minutes() {
        let line = |player: i32, year: i32, minutes: f64, worth_elo: f64| BoxSeason {
            player_id: PlayerId::new(player),
            season: season(year),
            minutes,
            worth_elo,
        };
        let boxes = career_box(&[
            line(1, 2010, 1000.0, 100.0),
            line(1, 2011, 3000.0, 40.0),
            line(2, 2010, 0.0, 300.0),
        ]);
        // (100 × 1000 + 40 × 3000) / 4000 = 55. A player without minutes has no box worth.
        assert_eq!(boxes.len(), 1);
        assert!((boxes[&PlayerId::new(1)] - 55.0).abs() < 1e-9);
    }

    #[test]
    fn the_calibration_finds_the_line_and_the_spread_beyond_the_errors() {
        let (careers, boxes) = balanced();
        let pairs: Vec<(f64, f64, f64)> = careers
            .iter()
            .map(|c| {
                let gap = c.gap.unwrap();
                (boxes[&c.player_id], gap.worth, gap.error)
            })
            .collect();
        let c = calibrate(&pairs).unwrap();
        assert_eq!(c.players, 30);
        assert!((c.intercept - 10.0).abs() < 1e-9 && (c.slope - 0.5).abs() < 1e-9);
        // Every residual is 30 and every error 18: 900 − 324 = 576 = 24².
        assert!((c.spread - 24.0).abs() < 1e-9);
        assert!((c.mean_box - 70.0).abs() < 1e-9);
        // Each weight is 1 / (324 + 576) = 1 / 900: the line's error at the centre is the
        // root of 900 / 30. The box worths' squares around 70 sum to 2 × 100 × 280 = 56,000.
        assert!((c.mean_error - 30.0f64.sqrt()).abs() < 1e-9);
        assert!((c.slope_error - (900.0f64 / 56_000.0).sqrt()).abs() < 1e-9);
        // At the centre: 10 + 35 = 45, with the root of 576 + 30. Seventy away, the slope's
        // error adds 4,900 × 900 / 56,000 = 78.75.
        let (mean, error) = c.prior(70.0);
        assert!((mean - 45.0).abs() < 1e-9 && (error - 606.0f64.sqrt()).abs() < 1e-9);
        let (mean, error) = c.prior(140.0);
        assert!((mean - 80.0).abs() < 1e-9 && (error - 684.75f64.sqrt()).abs() < 1e-9);
    }

    #[test]
    fn a_prior_never_claims_less_than_the_least_error() {
        // Every gap on the line, with errors: nothing is left beyond the errors.
        let pairs: Vec<(f64, f64, f64)> = (0..30)
            .map(|i| (f64::from(i) * 10.0, 10.0 + 5.0 * f64::from(i), 20.0))
            .collect();
        let c = calibrate(&pairs).unwrap();
        assert_eq!(c.spread, 0.0);
        // The line's error at the centre is the root of 400 / 30, under the least error.
        assert!((c.mean_error - (400.0f64 / 30.0).sqrt()).abs() < 1e-9);
        assert_eq!(c.prior(c.mean_box).1, MIN_PRIOR_ERROR);
    }

    #[test]
    fn too_few_players_or_one_box_worth_give_no_calibration() {
        let pairs: Vec<(f64, f64, f64)> = (0..29)
            .map(|i| (f64::from(i) * 10.0, f64::from(i), 20.0))
            .collect();
        assert_eq!(calibrate(&pairs), None);
        let flat: Vec<(f64, f64, f64)> = (0..40).map(|i| (80.0, f64::from(i), 20.0)).collect();
        assert_eq!(calibrate(&flat), None);
        // A gap without an error is not a point of the fit.
        let mut exact: Vec<(f64, f64, f64)> = (0..30)
            .map(|i| (f64::from(i) * 10.0, f64::from(i), 20.0))
            .collect();
        exact[0].2 = 0.0;
        assert_eq!(calibrate(&exact), None);
    }

    #[test]
    fn the_worth_weighs_the_prior_and_the_gap_by_their_precision() {
        let (mut careers, mut boxes) = balanced();
        // A player at the centre without a gap: he is not a point of the fit.
        careers.push(career(100, None));
        boxes.insert(PlayerId::new(100), 70.0);
        let r = rank(careers, &boxes, settings(), None, &mut rng(1));
        assert_eq!((r.measured, r.ranked, r.without_gap), (31, 31, 1));
        assert_eq!(r.calibration.unwrap().players, 30);
        // Player 14 has the box worth 70 and the gap 45 + 30 = 75 ± 18; his prior is
        // 45 with the variance 606. Precisions 1/606 and 1/324.
        let p = row(&r, 14);
        assert!((p.prior - 45.0).abs() < 1e-9 && (p.prior_error - 606.0f64.sqrt()).abs() < 1e-9);
        let (a, b) = (1.0 / 606.0, 1.0 / 324.0);
        assert!((p.worth - (45.0 * a + 75.0 * b) / (a + b)).abs() < 1e-9);
        assert!((p.error - (1.0 / (a + b)).sqrt()).abs() < 1e-9);
        // The player without a gap keeps the prior.
        let q = row(&r, 100);
        assert_eq!((q.worth, q.error), (q.prior, q.prior_error));
        assert!((q.worth - 45.0).abs() < 1e-9);
        assert!((r.typical_gap_error - 18.0).abs() < 1e-9);
    }

    #[test]
    fn a_short_career_or_one_without_a_box_worth_is_counted_and_not_ranked() {
        let (mut careers, mut boxes) = balanced();
        let mut brief = career(200, Some((300.0, 20.0)));
        brief.career_games = 120;
        careers.push(brief);
        boxes.insert(PlayerId::new(200), 150.0);
        careers.push(career(201, Some((300.0, 20.0))));
        let r = rank(careers, &boxes, settings(), None, &mut rng(1));
        assert_eq!(
            (r.measured, r.ranked, r.short_careers, r.without_box),
            (32, 30, 1, 1)
        );
        assert!(r
            .players
            .iter()
            .all(|row| ![200, 201].contains(&row.career.player_id.as_i32())));
    }

    #[test]
    fn without_a_calibration_no_player_is_ranked() {
        let careers: Vec<Career> = (0..10)
            .map(|i| career(i, Some((f64::from(i), 20.0))))
            .collect();
        let boxes: BTreeMap<PlayerId, f64> = (0..10)
            .map(|i| (PlayerId::new(i), f64::from(i) * 10.0))
            .collect();
        let r = rank(careers, &boxes, settings(), None, &mut rng(1));
        assert_eq!((r.measured, r.ranked, r.calibration), (10, 0, None));
        assert!(r.players.is_empty());
        let empty = rank(vec![], &BTreeMap::new(), settings(), None, &mut rng(1));
        assert_eq!((empty.measured, empty.ranked), (0, 0));
        assert_eq!((empty.spread, empty.typical_error), (0.0, 0.0));
    }

    /// A population with gaps scattered around a line of the box worths.
    fn scattered(n: i32) -> (Vec<Career>, BTreeMap<PlayerId, f64>) {
        let careers = (0..n)
            .map(|i| {
                let off = f64::from(i % 7) * 12.0 - 36.0;
                career(i, Some((f64::from(i) + off, 25.0 + f64::from(i % 5))))
            })
            .collect();
        let boxes = (0..n)
            .map(|i| (PlayerId::new(i), f64::from(i) * 2.0))
            .collect();
        (careers, boxes)
    }

    #[test]
    fn chances_nest_and_fill_each_group() {
        let (careers, boxes) = scattered(150);
        let r = rank(careers, &boxes, settings(), None, &mut rng(3));
        assert_eq!(r.players.len(), 150);
        for row in &r.players {
            assert!(row.top10 <= row.top25 && row.top25 <= row.top50 && row.top50 <= row.top100);
        }
        // Every draw puts exactly 100 of the 150 in the largest group, so the listed 100
        // hold most of that total and never more than all of it.
        assert!(r.expected_in_list > 50.0 && r.expected_in_list <= 100.0);
        assert_eq!(r.tier_sizes.iter().sum::<usize>(), 100);
        let ranks: Vec<usize> = r.players.iter().map(|row| row.rank).collect();
        assert_eq!(ranks, (1..=150).collect::<Vec<_>>());
        assert!(r.players.windows(2).all(|w| w[0].tier <= w[1].tier));
        // A worth is never less certain than its prior or its gap.
        for row in &r.players {
            assert!(row.error <= row.prior_error && row.error <= row.career.gap.unwrap().error);
        }
    }

    #[test]
    fn the_same_seed_gives_the_same_ranking() {
        let run = |seed: u64| {
            let (careers, boxes) = scattered(60);
            rank(careers, &boxes, settings(), None, &mut rng(seed))
        };
        let (a, b, c) = (run(11), run(11), run(12));
        assert_eq!(a, b);
        assert_ne!(a.players, c.players);
    }

    #[test]
    fn a_tier_is_the_smallest_group_at_an_even_chance() {
        assert_eq!(Tier::of([0.5, 0.9, 1.0, 1.0]), Tier::One);
        assert_eq!(Tier::of([0.49, 0.5, 1.0, 1.0]), Tier::Two);
        assert_eq!(Tier::of([0.0, 0.2, 0.7, 1.0]), Tier::Three);
        assert_eq!(Tier::of([0.0, 0.0, 0.3, 0.6]), Tier::Four);
        assert_eq!(Tier::of([0.0, 0.0, 0.1, 0.49]), Tier::Five);
        assert_eq!(serde_json::to_value(Tier::Three).unwrap(), 3);
    }

    #[test]
    fn a_score_reads_the_misses_against_the_errors() {
        // Targets 0, 2, 4, each with the error 1; the estimate in place 0 says −3, 2, 7 with
        // the error √8. The misses are 3, 0, −3.
        let pairs: Vec<Scored> = [(0.0, -3.0), (2.0, 2.0), (4.0, 7.0)]
            .into_iter()
            .map(|(target, estimate)| Scored {
                target,
                target_error: 1.0,
                estimates: [(estimate, 8.0f64.sqrt()), (0.0, 1.0), (target, 0.0)],
            })
            .collect();
        let s = score(&pairs, 0);
        assert!((s.correlation - 1.0).abs() < 1e-9);
        // The mean squared miss is 6, of which 1 is the targets' own error.
        assert!((s.excess_error - 5.0f64.sqrt()).abs() < 1e-9);
        // Each miss over the root of 1 + 8: 1, 0, −1.
        assert!((s.z_spread - (2.0f64 / 3.0).sqrt()).abs() < 1e-9);
        // An estimate that does not move has no correlation; one that is the target has no
        // miss at all.
        assert_eq!(score(&pairs, 1).correlation, 0.0);
        let exact = score(&pairs, 2);
        assert_eq!((exact.excess_error, exact.z_spread), (0.0, 0.0));
    }

    #[test]
    fn the_halves_agree_when_the_gap_follows_the_box_worth() {
        // Each player's gap is 10 + half his box worth in an odd and in an even season.
        let mut cells = Vec::new();
        let mut boxes = Vec::new();
        for player in 0..30 {
            let box_worth = f64::from(player) * 4.0;
            let gap = 10.0 + 0.5 * box_worth;
            for year in [2010, 2011] {
                cells.extend(stint(
                    player,
                    year,
                    1,
                    &[gap + 1.0, gap - 1.0, gap],
                    &[1.0, -1.0, 0.0],
                ));
                boxes.push(BoxSeason {
                    player_id: PlayerId::new(player),
                    season: season(year),
                    minutes: 2000.0,
                    worth_elo: box_worth,
                });
            }
        }
        let check = half_check(&cells, &boxes, 0, 1.0).unwrap();
        assert_eq!(check.pairs, 60);
        for s in [check.gap, check.prior, check.blend] {
            assert!((s.correlation - 1.0).abs() < 1e-9);
        }
        // The prior of one half is the gap of the other, to the last digit.
        assert!(check.prior.excess_error < 1e-9 && check.blend.excess_error < 1e-9);
        // A half with fewer players than a calibration needs gives no check, and so does a
        // floor of games no player reaches.
        assert_eq!(half_check(&cells[..80], &boxes, 0, 1.0), None);
        assert_eq!(half_check(&cells, &boxes, 100, 1.0), None);
    }
}
