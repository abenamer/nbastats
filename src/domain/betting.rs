//! The betting portfolio: a pot staked on the favourite of every game the sportsbook priced,
//! read as a stock portfolio is. Three strategies run side by side from a start date: the
//! sportsbook's favourite, the model's favourite, and the model's favourite only where it
//! differs from the sportsbook's (the bettor sits out the rest). Each runs at the posted
//! prices and at the fair ones, the sportsbook's moneylines with the bookmaker's margin taken
//! out, so the margin's cost can be read apart from the forecasts' worth. The stake is the
//! same on every game and the pot is the running sum of the returns, so a run never stops for
//! want of money: the path is the running total of a flat bet.
//!
//! Everything here is pure: the input is the record's scored snapshots
//! (`domain::tracking::Scored`), the output the paths, the performance of each run and a
//! row per season.

use chrono::{Duration, NaiveDate};
use serde::Serialize;

use crate::domain::ids::SeasonYear;
use crate::domain::market::{self, Side};
use crate::domain::tracking::Scored;

/// The pot a portfolio starts with unless asked otherwise.
pub const DEFAULT_POT: f64 = 10_000.0;
/// The stake on each game unless asked otherwise.
pub const DEFAULT_STAKE: f64 = 100.0;

/// Whose favourite the stake goes on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Strategy {
    /// The side the sportsbook's margin-free chance puts at one half or above.
    MarketFavourite,
    /// The side the model's chance puts at one half or above.
    ModelFavourite,
    /// The model's favourite on the games where it is not the sportsbook's; no bet on the
    /// rest.
    ModelDisagreements,
}

/// The price a bet is settled at.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Price {
    /// The sportsbook's moneyline as posted, margin included.
    Posted,
    /// The sportsbook's margin-free chance of the side, as a moneyline: a win on a side with
    /// chance p returns (1 − p) / p per unit staked.
    Fair,
}

/// A strategy at a price.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Run {
    pub strategy: Strategy,
    pub price: Price,
}

/// Every run, in the order the paths and the season rows carry them.
pub const RUNS: [Run; 6] = [
    Run {
        strategy: Strategy::MarketFavourite,
        price: Price::Posted,
    },
    Run {
        strategy: Strategy::ModelFavourite,
        price: Price::Posted,
    },
    Run {
        strategy: Strategy::ModelDisagreements,
        price: Price::Posted,
    },
    Run {
        strategy: Strategy::MarketFavourite,
        price: Price::Fair,
    },
    Run {
        strategy: Strategy::ModelFavourite,
        price: Price::Fair,
    },
    Run {
        strategy: Strategy::ModelDisagreements,
        price: Price::Fair,
    },
];

/// What a portfolio is run with.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Settings {
    /// The first date bet on; none for the first game of the record.
    pub start: Option<NaiveDate>,
    pub pot: f64,
    pub stake: f64,
}

/// The pot of every run at the end of a day, in the order of [`RUNS`]. The first day is the
/// day before the first bet, at the starting pot.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Day {
    pub date: NaiveDate,
    pub values: Vec<f64>,
}

/// How a run did.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Performance {
    pub run: Run,
    pub bets: usize,
    pub wins: usize,
    /// The final pot less the starting one.
    pub net: f64,
    /// Its standard error: the sample standard deviation of the bets' returns times the
    /// square root of their count; none under two bets.
    pub net_se: Option<f64>,
    pub final_value: f64,
    /// The net over the starting pot.
    pub return_share: f64,
    pub peak: f64,
    /// The largest fall from a peak to a later value, in money and as a share of that peak.
    pub max_drawdown: f64,
    pub max_drawdown_share: f64,
}

/// One run's bets in a season.
#[derive(Debug, Clone, PartialEq, Serialize, Default)]
pub struct Tally {
    pub bets: usize,
    pub wins: usize,
    pub net: f64,
}

/// A season's games from the start date, with each run's tally in the order of [`RUNS`].
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SeasonRow {
    pub season: SeasonYear,
    pub games: usize,
    pub runs: Vec<Tally>,
}

/// The portfolio: the paths, each run's performance and the rows per season.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Portfolio {
    pub settings: Settings,
    /// The first and last dates bet on; none without a game.
    pub first: Option<NaiveDate>,
    pub last: Option<NaiveDate>,
    /// Games bet on: priced by the sportsbook, played, on or after the start date.
    pub games: usize,
    /// Scored games left out for want of a sportsbook chance or a moneyline.
    pub unpriced: usize,
    /// Scored games before the start date.
    pub before_start: usize,
    /// The sportsbook's mean margin over the games bet on.
    pub mean_margin: Option<f64>,
    pub runs: Vec<Performance>,
    pub days: Vec<Day>,
    pub seasons: Vec<SeasonRow>,
}

/// Each run's bet on a game as (won, return per unit staked), in the order of [`RUNS`]; a
/// run that sits the game out has none. The game itself has none without a sportsbook
/// chance or a moneyline for both sides.
fn returns_of(g: &Scored) -> Option<[Option<(bool, f64)>; 6]> {
    let s = &g.snapshot;
    let p_home = s.p_home_market?;
    let home_pay = market::payout(s.home_moneyline?)?;
    let away_pay = market::payout(s.away_moneyline?)?;
    let market_side = if p_home >= 0.5 {
        Side::Home
    } else {
        Side::Away
    };
    let model_side = if s.p_home_model >= 0.5 {
        Side::Home
    } else {
        Side::Away
    };
    let bet = |side: Side| {
        let won = (side == Side::Home) == g.home_won;
        let (pay, p) = match side {
            Side::Home => (home_pay, p_home),
            Side::Away => (away_pay, 1.0 - p_home),
        };
        // Held inside (0, 1) as the fair moneyline is, so a certainty stays finite.
        let p = p.clamp(0.001, 0.999);
        let posted = if won { pay } else { -1.0 };
        let fair = if won { (1.0 - p) / p } else { -1.0 };
        ((won, posted), (won, fair))
    };
    let (market_posted, market_fair) = bet(market_side);
    let (model_posted, model_fair) = bet(model_side);
    let disagree = market_side != model_side;
    Some([
        Some(market_posted),
        Some(model_posted),
        disagree.then_some(model_posted),
        Some(market_fair),
        Some(model_fair),
        disagree.then_some(model_fair),
    ])
}

/// The portfolio of `scored` under `settings`. The games are taken in date order whatever
/// order they come in.
pub fn portfolio(scored: &[Scored], settings: &Settings) -> Portfolio {
    let mut games: Vec<&Scored> = scored.iter().collect();
    games.sort_by(|a, b| {
        (
            a.snapshot.game_date,
            &a.snapshot.home_code,
            &a.snapshot.away_code,
        )
            .cmp(&(
                b.snapshot.game_date,
                &b.snapshot.home_code,
                &b.snapshot.away_code,
            ))
    });
    let n = RUNS.len();
    let mut value = vec![settings.pot; n];
    let mut peak = vec![settings.pot; n];
    let mut drawdown = vec![0.0; n];
    let mut drawdown_share = vec![0.0; n];
    let mut bets = vec![0usize; n];
    let mut wins = vec![0usize; n];
    let mut sum = vec![0.0; n];
    let mut sum_sq = vec![0.0; n];
    let mut days: Vec<Day> = Vec::new();
    let mut seasons: Vec<SeasonRow> = Vec::new();
    let (mut first, mut last) = (None, None);
    let (mut count, mut unpriced, mut before_start) = (0usize, 0usize, 0usize);
    let (mut margin_sum, mut margin_count) = (0.0, 0usize);
    for g in games {
        let date = g.snapshot.game_date;
        if settings.start.is_some_and(|start| date < start) {
            before_start += 1;
            continue;
        }
        let Some(returns) = returns_of(g) else {
            unpriced += 1;
            continue;
        };
        if first.is_none() {
            first = Some(date);
            days.push(Day {
                date: settings.start.unwrap_or(date) - Duration::days(1),
                values: value.clone(),
            });
        }
        last = Some(date);
        count += 1;
        if let Some(m) = g.snapshot.book_margin {
            margin_sum += m;
            margin_count += 1;
        }
        let season = g.snapshot.season;
        let row = match seasons.iter().position(|r| r.season == season) {
            Some(i) => i,
            None => {
                seasons.push(SeasonRow {
                    season,
                    games: 0,
                    runs: vec![Tally::default(); n],
                });
                seasons.len() - 1
            }
        };
        seasons[row].games += 1;
        for (i, r) in returns.iter().enumerate() {
            let Some((won, units)) = r else { continue };
            let money = units * settings.stake;
            bets[i] += 1;
            wins[i] += usize::from(*won);
            sum[i] += money;
            sum_sq[i] += money * money;
            value[i] += money;
            peak[i] = peak[i].max(value[i]);
            let fall = peak[i] - value[i];
            if fall > drawdown[i] {
                drawdown[i] = fall;
                drawdown_share[i] = if peak[i] > 0.0 { fall / peak[i] } else { 0.0 };
            }
            let tally = &mut seasons[row].runs[i];
            tally.bets += 1;
            tally.wins += usize::from(*won);
            tally.net += money;
        }
        match days.last_mut() {
            Some(d) if d.date == date => d.values.clone_from(&value),
            _ => days.push(Day {
                date,
                values: value.clone(),
            }),
        }
    }
    let runs = RUNS
        .iter()
        .enumerate()
        .map(|(i, run)| {
            let k = bets[i] as f64;
            let net_se = (bets[i] >= 2).then(|| {
                let variance = (sum_sq[i] - sum[i] * sum[i] / k) / (k - 1.0);
                variance.max(0.0).sqrt() * k.sqrt()
            });
            Performance {
                run: *run,
                bets: bets[i],
                wins: wins[i],
                net: value[i] - settings.pot,
                net_se,
                final_value: value[i],
                return_share: if settings.pot > 0.0 {
                    (value[i] - settings.pot) / settings.pot
                } else {
                    0.0
                },
                peak: peak[i],
                max_drawdown: drawdown[i],
                max_drawdown_share: drawdown_share[i],
            }
        })
        .collect();
    Portfolio {
        settings: *settings,
        first,
        last,
        games: count,
        unpriced,
        before_start,
        mean_margin: (margin_count > 0).then(|| margin_sum / margin_count as f64),
        runs,
        days,
        seasons,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::tracking::{score, Outcome, Snapshot};
    use chrono::{DateTime, Utc};

    fn day(d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 11, d).unwrap()
    }

    /// A scored game: the model's and the sportsbook's home chance, the two moneylines and
    /// whether the home side won; no sportsbook chance when `p_market` is none.
    fn game(
        date: NaiveDate,
        p_model: f64,
        p_market: Option<f64>,
        lines: (f64, f64),
        home_won: bool,
    ) -> Scored {
        let snapshot = Snapshot {
            season: SeasonYear::of(2027),
            game_date: date,
            home_code: "GSW".into(),
            away_code: "LAC".into(),
            captured_at: DateTime::<Utc>::from_timestamp(0, 0).unwrap(),
            start_time: None,
            model_version: "m1".into(),
            app_version: "0.1.0".into(),
            home_advantage: 41.0,
            home_rating: 1500.0,
            away_rating: 1500.0,
            home_stored: 1500.0,
            away_stored: 1500.0,
            home_penalty: 0.0,
            away_penalty: 0.0,
            p_home_full: p_model,
            p_home_model: p_model,
            home_margin_model: 0.0,
            provider: "FanDuel".into(),
            source: "the-odds-api".into(),
            home_spread: None,
            home_spread_odds: None,
            away_spread_odds: None,
            home_moneyline: Some(lines.0),
            away_moneyline: Some(lines.1),
            total: None,
            p_home_market: p_market,
            book_margin: Some(0.04),
        };
        let (home_score, away_score) = if home_won { (110, 100) } else { (100, 110) };
        score(
            snapshot,
            Outcome {
                home_score,
                away_score,
            },
            12.0,
            0.05,
        )
        .unwrap()
    }

    fn settings(start: Option<NaiveDate>) -> Settings {
        Settings {
            start,
            pot: DEFAULT_POT,
            stake: DEFAULT_STAKE,
        }
    }

    fn by(p: &Portfolio, strategy: Strategy, price: Price) -> &Performance {
        p.runs
            .iter()
            .find(|r| r.run.strategy == strategy && r.run.price == price)
            .unwrap()
    }

    #[test]
    fn a_favourite_that_wins_returns_the_posted_payout_and_the_fair_one() {
        // Both favour the home side at 60%, priced -150: a $100 win returns $66.67 as
        // posted and $66.67 at the fair price too, since 0.4 / 0.6 is the same odds.
        let p = portfolio(
            &[game(day(1), 0.6, Some(0.6), (-150.0, 130.0), true)],
            &settings(None),
        );
        assert_eq!(p.games, 1);
        let market = by(&p, Strategy::MarketFavourite, Price::Posted);
        assert_eq!((market.bets, market.wins), (1, 1));
        assert!((market.net - 10_000.0 / 150.0).abs() < 1e-9);
        assert!((market.final_value - (DEFAULT_POT + 10_000.0 / 150.0)).abs() < 1e-9);
        assert_eq!(market.net_se, None);
        let fair = by(&p, Strategy::MarketFavourite, Price::Fair);
        assert!((fair.net - 100.0 * 0.4 / 0.6).abs() < 1e-9);
        // The two agree, so the disagreement run sits out.
        let sits = by(&p, Strategy::ModelDisagreements, Price::Posted);
        assert_eq!((sits.bets, sits.net), (0, 0.0));
        assert!((p.mean_margin.unwrap() - 0.04).abs() < 1e-12);
    }

    #[test]
    fn a_disagreement_is_bet_on_the_models_side_at_the_sportsbooks_price_for_it() {
        // The sportsbook has the home side at 60%, the model the away side; the away side
        // wins at +130: $130 as posted, $150 at the fair price (0.6 / 0.4).
        let p = portfolio(
            &[game(day(1), 0.45, Some(0.6), (-150.0, 130.0), false)],
            &settings(None),
        );
        assert!((by(&p, Strategy::ModelFavourite, Price::Posted).net - 130.0).abs() < 1e-9);
        assert!((by(&p, Strategy::ModelFavourite, Price::Fair).net - 150.0).abs() < 1e-9);
        let d = by(&p, Strategy::ModelDisagreements, Price::Posted);
        assert_eq!((d.bets, d.wins), (1, 1));
        assert!((d.net - 130.0).abs() < 1e-9);
        // The sportsbook's favourite lost its stake.
        assert!((by(&p, Strategy::MarketFavourite, Price::Posted).net + 100.0).abs() < 1e-9);
    }

    #[test]
    fn the_path_runs_day_by_day_from_the_pot_and_the_drawdown_is_the_fall_from_the_peak() {
        // Win, lose, lose, win on the sportsbook's favourite at -110, two games on the
        // second day.
        let games = [
            game(day(1), 0.6, Some(0.6), (-110.0, -110.0), true),
            game(day(2), 0.6, Some(0.6), (-110.0, -110.0), false),
            game(day(2), 0.6, Some(0.6), (-110.0, -110.0), false),
            game(day(4), 0.6, Some(0.6), (-110.0, -110.0), true),
        ];
        let p = portfolio(&games, &settings(None));
        let pay = 10_000.0 / 110.0;
        assert_eq!(
            p.days.len(),
            4,
            "the day before the first bet and three bet days"
        );
        assert_eq!(p.days[0].date, day(1) - Duration::days(1));
        assert!(p.days[0].values.iter().all(|v| *v == DEFAULT_POT));
        assert!((p.days[1].values[0] - (DEFAULT_POT + pay)).abs() < 1e-9);
        assert!((p.days[2].values[0] - (DEFAULT_POT + pay - 200.0)).abs() < 1e-9);
        assert!((p.days[3].values[0] - (DEFAULT_POT + 2.0 * pay - 200.0)).abs() < 1e-9);
        let m = by(&p, Strategy::MarketFavourite, Price::Posted);
        assert_eq!((m.bets, m.wins), (4, 2));
        assert!((m.peak - (DEFAULT_POT + pay)).abs() < 1e-9);
        assert!((m.max_drawdown - 200.0).abs() < 1e-9);
        assert!((m.max_drawdown_share - 200.0 / (DEFAULT_POT + pay)).abs() < 1e-12);
        assert!((m.return_share - m.net / DEFAULT_POT).abs() < 1e-12);
        // The error of the net: returns of +90.91, -100, -100, +90.91 have a sample
        // standard deviation of about 110.2, times two.
        let se = m.net_se.unwrap();
        assert!((se - 2.0 * 110.219).abs() < 0.01, "{se}");
        assert_eq!((p.first, p.last), (Some(day(1)), Some(day(4))));
        assert_eq!(p.seasons.len(), 1);
        assert_eq!(p.seasons[0].games, 4);
        assert_eq!(p.seasons[0].runs[0].wins, 2);
    }

    #[test]
    fn games_before_the_start_and_games_without_a_price_are_left_out() {
        let games = [
            game(day(1), 0.6, Some(0.6), (-150.0, 130.0), true),
            game(day(3), 0.6, None, (-150.0, 130.0), true),
            game(day(5), 0.6, Some(0.6), (-150.0, 130.0), true),
        ];
        let p = portfolio(&games, &settings(Some(day(2))));
        assert_eq!((p.games, p.unpriced, p.before_start), (1, 1, 1));
        assert_eq!(p.first, Some(day(5)));
        assert_eq!(p.days[0].date, day(1), "the day before the start date");
        assert_eq!(p.days.len(), 2);
        // Nothing at all: no day, no game.
        let empty = portfolio(&[], &settings(None));
        assert!(empty.days.is_empty() && empty.runs.len() == RUNS.len());
        assert_eq!(empty.mean_margin, None);
    }
}
