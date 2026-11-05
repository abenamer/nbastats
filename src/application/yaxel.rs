//! Use case: the YAXEL backtest. Builds the samples of each stored season from the games,
//! the box scores and the worths the players entered the season with, and scores the
//! emergence rule with `domain::yaxel`. Read-only: nothing here touches a rating or a page.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;

use chrono::{Duration, NaiveDate};
use serde::Serialize;

use crate::domain::bpm::{self, BoxLine, BoxTable};
use crate::domain::ids::{PlayerId, SeasonYear, TeamId};
use crate::domain::models::{Game, GameType};
use crate::domain::worth::{BoxCalibration, WorthTable};
use crate::domain::yaxel::{self, Floor, GameSample, Grid, Report, SeasonSample, Strength};

use super::boxscore;
use super::ports::PlayerStore;
use super::rookie as app_rookie;
use super::state::{App, SeasonLoadError};
use super::worth::{self, Pricer, WORTH_LOOKBACK};

/// The early-season cut by default: games in which neither team has played more than this
/// many.
pub const DEFAULT_THROUGH_GAME: u32 = 30;
/// The fewest played regular-season games for a season to be sampled.
pub const MIN_GAMES: usize = 100;

/// A season left out, and why.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Skipped {
    pub season: SeasonYear,
    pub reason: String,
}

/// The backtest's answer: the report when at least two seasons were sampled, the seasons
/// left out, and a note when there is no report.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Backtest {
    pub from: SeasonYear,
    pub to: SeasonYear,
    pub report: Option<Report>,
    pub skipped: Vec<Skipped>,
    pub note: Option<String>,
}

/// Backtest the rule over the seasons `from` to `to`. `progress` hears each season as it
/// is sampled.
pub async fn backtest(
    app: &App,
    from: SeasonYear,
    to: SeasonYear,
    grid: &Grid,
    through_game: u32,
    new_minutes: f64,
    progress: &(dyn Fn(&str) + Sync),
) -> Result<Backtest, SeasonLoadError> {
    let mut samples = Vec::new();
    let mut skipped = Vec::new();
    for season in SeasonYear::range(from, to) {
        match season_sample(app, season, grid).await? {
            Ok(sample) => {
                progress(&format!(
                    "{}: {} games sampled{}",
                    season.label(),
                    sample.games.len(),
                    if sample.new_known {
                        ""
                    } else {
                        "; new players not told, no earlier season of box scores to price from"
                    }
                ));
                samples.push(sample);
            }
            Err(reason) => {
                progress(&format!("{}: skipped, {reason}", season.label()));
                skipped.push(Skipped { season, reason });
            }
        }
    }
    let report = yaxel::backtest(&samples, grid, through_game, new_minutes);
    let note = report.is_none().then(|| {
        format!(
            "{} season(s) sampled; the backtest needs two.",
            samples.len()
        )
    });
    Ok(Backtest {
        from,
        to,
        report,
        skipped,
        note,
    })
}

/// The worth and box tables of the seasons before `season`, latest first, for the pricer.
async fn entering_tables(
    app: &App,
    season: SeasonYear,
) -> Result<
    (
        Vec<(SeasonYear, Arc<WorthTable>)>,
        Vec<(SeasonYear, Arc<BoxTable>)>,
    ),
    SeasonLoadError,
> {
    let mut tables = Vec::new();
    let mut box_tables = Vec::new();
    let mut s = season.previous();
    for _ in 0..=WORTH_LOOKBACK {
        let Some(current) = s else { break };
        match worth::table(app, current).await {
            Ok(t) => tables.push((current, t)),
            Err(SeasonLoadError::NoGames(_)) => {}
            Err(e) => return Err(e),
        }
        match boxscore::table(app, current).await {
            Ok(t) if t.bpm_available => box_tables.push((current, t)),
            Ok(_) | Err(SeasonLoadError::NoGames(_)) => {}
            Err(e) => return Err(e),
        }
        s = current.previous();
    }
    Ok((tables, box_tables))
}

/// A season's sample, or the reason it has none.
async fn season_sample(
    app: &App,
    season: SeasonYear,
    grid: &Grid,
) -> Result<Result<SeasonSample, String>, SeasonLoadError> {
    let data = match app.season_data(season).await {
        Ok(d) => d,
        Err(SeasonLoadError::NoGames(_)) => return Ok(Err("no games".to_string())),
        Err(e) => return Err(e),
    };
    let mut games: Vec<Game> = data
        .games
        .iter()
        .filter(|g| g.game_type == GameType::Regular && g.is_played())
        .cloned()
        .collect();
    games.sort_by_key(|g| (g.game_date, g.seq));
    if games.len() < MIN_GAMES {
        return Ok(Err(format!(
            "{} played regular-season games, under {MIN_GAMES}",
            games.len()
        )));
    }
    let season_box = boxscore::table(app, season).await?;
    if !season_box.bpm_available {
        return Ok(Err("box scores without BPM".to_string()));
    }
    let lines = app.store.season_box_lines(season).await?;
    let mut ids: Vec<PlayerId> = lines.iter().map(|l| l.player_id).collect();
    ids.sort_unstable();
    ids.dedup();
    let players = app.store.players_by_id(&ids).await?;
    let listed: HashMap<PlayerId, f64> = players
        .iter()
        .filter_map(|p| {
            p.position
                .as_deref()
                .and_then(bpm::listed_position)
                .map(|v| (p.id, v))
        })
        .collect();
    let (tables, box_tables) = entering_tables(app, season).await?;
    if tables.is_empty() && box_tables.is_empty() {
        return Ok(Err(
            "no earlier season to price the players from".to_string()
        ));
    }
    // Without a season of box scores among the ones the players are priced from, nobody
    // has an earlier line, so every player drafted within five years, or without a draft
    // year, would look new: the season cannot tell its new players.
    let new_known = !box_tables.is_empty();
    let pricer = Pricer::load(app, season, tables, box_tables, &ids).await?;
    // The worth each player entered the season with, and its error: the floors' priors
    // and the price of an absence alike. A player without an estimate is left out.
    let entering: HashMap<PlayerId, (f64, f64)> = ids
        .iter()
        .filter_map(|&id| {
            let (estimate, _) = pricer.estimate(id, pricer.players.get(&id));
            estimate.map(|e| (id, (e.elo, e.se_elo)))
        })
        .collect();
    // The players new to the league this season; one without a record counts as new.
    let new_players: HashSet<PlayerId> = ids
        .iter()
        .copied()
        .filter(|id| pricer.is_new(*id, pricer.players.get(id)))
        .collect();
    let calibration = pricer.calibration.clone();
    let carried = carried_rosters(app, season).await?;
    let home_advantage = app.home_advantage(season).await?.regular;
    let grid = grid.clone();
    let mut sample = tokio::task::spawn_blocking(move || {
        build_season(
            season,
            home_advantage,
            &games,
            &lines,
            &listed,
            &entering,
            &carried,
            &new_players,
            calibration.as_ref(),
            &grid,
        )
    })
    .await
    .map_err(|e| {
        SeasonLoadError::Store(super::ports::StoreError::Query(format!(
            "the sample did not build: {e}"
        )))
    })?;
    sample.new_known = new_known;
    Ok(Ok(sample))
}

/// The team each player ended the season before `season` with, by his last line of it
/// (`yaxel::last_teams`), which puts him on its roster from the season's start. Empty
/// without such a season.
async fn carried_rosters(
    app: &App,
    season: SeasonYear,
) -> Result<HashMap<PlayerId, TeamId>, SeasonLoadError> {
    let Some(previous) = season.previous() else {
        return Ok(HashMap::new());
    };
    let data = match app.season_data(previous).await {
        Ok(d) => d,
        Err(SeasonLoadError::NoGames(_)) => return Ok(HashMap::new()),
        Err(e) => return Err(e),
    };
    let lines = app.store.season_box_lines(previous).await?;
    Ok(yaxel::last_teams(&data.games, &lines))
}

/// The games of a season sampled: per date, the floors of the teams playing are the players
/// who played in each team's previous game, each with the worth he entered the season with
/// (`entering`, with its error), his box worth over the window before the date, whether he
/// is new to the league and his minutes in that previous game; and each side's strength as
/// the live forecast would read it, the absences read from the box scores at the entering
/// worths, with `carried` the team each player ended the season before with
/// (`yaxel::box_absences`, `yaxel::strengths`).
#[allow(clippy::too_many_arguments)]
fn build_season(
    season: SeasonYear,
    home_advantage: f64,
    games: &[Game],
    lines: &[BoxLine],
    listed: &HashMap<PlayerId, f64>,
    entering: &HashMap<PlayerId, (f64, f64)>,
    carried: &HashMap<PlayerId, TeamId>,
    new_players: &HashSet<PlayerId>,
    calibration: Option<&BoxCalibration>,
    grid: &Grid,
) -> SeasonSample {
    let strengths = yaxel::strengths(games, &yaxel::box_absences(games, lines, entering, carried));
    let mut by_game: HashMap<i64, Vec<&BoxLine>> = HashMap::new();
    for l in lines {
        by_game.entry(l.game_id).or_default().push(l);
    }
    let mut by_date: BTreeMap<NaiveDate, Vec<&Game>> = BTreeMap::new();
    for g in games {
        by_date.entry(g.game_date).or_default().push(g);
    }
    // The players who played for a team in a game, with their minutes.
    let played = |game: &Game, team: TeamId| -> Vec<(PlayerId, f64)> {
        by_game
            .get(&game.id)
            .map(|ls| {
                ls.iter()
                    .filter(|l| l.team_id == team)
                    .filter_map(|l| {
                        l.seconds
                            .filter(|s| *s > 0)
                            .map(|s| (l.player_id, f64::from(s) / 60.0))
                    })
                    .collect()
            })
            .unwrap_or_default()
    };
    let mut last_floor: HashMap<TeamId, Vec<(PlayerId, f64)>> = HashMap::new();
    let mut game_no: HashMap<TeamId, u32> = HashMap::new();
    let mut out = Vec::with_capacity(games.len());
    for (&date, todays) in &by_date {
        // The window tables of the date: one per window of the grid, from the games played
        // in the window before the date.
        let tables: Vec<BoxTable> = grid
            .window_days
            .iter()
            .map(|&w| {
                let start = date - Duration::days(i64::from(w));
                let window: Vec<Game> = games
                    .iter()
                    .filter(|g| g.game_date >= start && g.game_date < date)
                    .cloned()
                    .collect();
                let window_lines: Vec<BoxLine> = window
                    .iter()
                    .flat_map(|g| by_game.get(&g.id).into_iter().flatten())
                    .map(|l| **l)
                    .collect();
                bpm::table(season, &window, &window_lines, listed)
            })
            .collect();
        let floor_of = |team: TeamId, table: &BoxTable| -> Vec<Floor> {
            last_floor
                .get(&team)
                .map(|players| {
                    players
                        .iter()
                        .map(|&(p, last_minutes)| Floor {
                            prior_elo: entering.get(&p).map_or(0.0, |e| e.0),
                            recent: table
                                .bpm_available
                                .then(|| app_rookie::season_box(table, p))
                                .flatten()
                                .map(|(box_elo, minutes)| {
                                    (calibration.map_or(box_elo, |c| c.prior(box_elo).0), minutes)
                                }),
                            new: new_players.contains(&p),
                            last_minutes,
                        })
                        .collect()
                })
                .unwrap_or_default()
        };
        for g in todays {
            let Some(outcome) = g.outcome() else { continue };
            let home_no = game_no.get(&g.home_team_id).copied().unwrap_or(0) + 1;
            let away_no = game_no.get(&g.away_team_id).copied().unwrap_or(0) + 1;
            let (home_strength, away_strength) = strengths.get(&g.id).copied().unwrap_or((
                Strength::plain(g.home_elo_pre),
                Strength::plain(g.away_elo_pre),
            ));
            out.push(GameSample {
                game_date: date,
                home_game_no: home_no,
                away_game_no: away_no,
                home_elo_pre: g.home_elo_pre,
                away_elo_pre: g.away_elo_pre,
                home_strength,
                away_strength,
                neutral: g.neutral,
                home_won: outcome.home_won(),
                floors: tables
                    .iter()
                    .map(|t| (floor_of(g.home_team_id, t), floor_of(g.away_team_id, t)))
                    .collect(),
            });
        }
        for g in todays {
            *game_no.entry(g.home_team_id).or_insert(0) += 1;
            *game_no.entry(g.away_team_id).or_insert(0) += 1;
            last_floor.insert(g.home_team_id, played(g, g.home_team_id));
            last_floor.insert(g.away_team_id, played(g, g.away_team_id));
        }
    }
    SeasonSample {
        season,
        home_advantage,
        games: out,
        new_known: true,
    }
}

fn signed(x: f64, digits: usize) -> String {
    format!("{}{:.*}", if x >= 0.0 { "+" } else { "" }, digits, x)
}

fn scored_cells(s: Option<&yaxel::Scored>) -> String {
    match s {
        Some(s) => format!(
            "{} | {:.4} | {:.4} | {} | {}",
            s.games,
            s.brier_base,
            s.brier_yaxel,
            signed(s.gain * 1000.0, 2),
            s.z.map_or("–".to_string(), |z| format!("{z:.1}"))
        ),
        None => "0 | – | – | – | –".to_string(),
    }
}

fn dash() -> String {
    "–".to_string()
}

fn cut_cells(c: &yaxel::Cut) -> String {
    let f4 = |x: Option<f64>| x.map_or_else(dash, |v| format!("{v:.4}"));
    format!(
        "{} | {} | {} | {} | {} | {} | {} | {}",
        c.games,
        if c.of == 0 {
            dash()
        } else {
            format!("{:.0}%", 100.0 * f64::from(c.games) / f64::from(c.of))
        },
        f4(c.elo.brier_new),
        f4(c.elo.brier_rest),
        c.team_games,
        c.elo.bias.map_or_else(dash, |b| signed(100.0 * b, 1)),
        c.elo.bias_elo().map_or_else(dash, |e| signed(e, 0)),
        c.elo.bias_z.map_or_else(dash, |z| format!("{z:.1}"))
    )
}

/// The cut's eight cells: their heads, their rule, and the cells of a season that cannot
/// tell its new players.
const CUT_HEADER: &str =
    "| Games with | Share | Brier (new) | Brier (rest) | Team-games | Bias (pts) | ≈ Elo | z |";
const CUT_RULE: &str = "|---|---|---|---|---|---|---|---|";
const NOT_TOLD: &str = "not told | – | – | – | – | – | – | –";

/// A forecast's bias on the cut in one cell: points of win chance, Elo at even odds and
/// standard errors.
fn bias_cell(b: &yaxel::Baseline) -> String {
    match (b.bias, b.bias_elo()) {
        (Some(p), Some(e)) => format!(
            "{} pts, ≈ {} Elo, z {}",
            signed(100.0 * p, 1),
            signed(e, 0),
            b.bias_z.map_or_else(dash, |z| format!("{z:.1}"))
        ),
        _ => dash(),
    }
}

/// The cut against the absence-penalized rating: the team-games, the Elo missing tonight on
/// the new players' teams and on the rest, and the bias against each of the three forecasts.
fn penalized_cells(c: &yaxel::Cut) -> String {
    let elo = |x: Option<f64>| x.map_or_else(dash, |v| format!("{v:.0}"));
    format!(
        "{} | {} / {} | {} | {} | {}",
        c.team_games,
        elo(c.missing_new),
        elo(c.missing_rest),
        bias_cell(&c.elo),
        bias_cell(&c.penalized_last),
        bias_cell(&c.penalized_tonight)
    )
}

const PENALIZED_HEADER: &str = "| Team-games | Missing Elo tonight, new / rest | Bias against Elo | Against the rating less the last game's absentees | Against the rating less tonight's absentees |";
const PENALIZED_RULE: &str = "|---|---|---|---|---|";
const PENALIZED_NOT_TOLD: &str = "not told | – | – | – | –";

/// The Brier scores of the three forecasts over all the games of a cut, in one sentence.
fn brier_sentence(label: &str, c: &yaxel::Cut) -> String {
    let f4 = |x: Option<f64>| x.map_or_else(dash, |v| format!("{v:.4}"));
    format!(
        "{label}: Elo {}; the rating less the last game's absentees {}; less tonight's {}.",
        f4(c.elo.brier_all),
        f4(c.penalized_last.brier_all),
        f4(c.penalized_tonight.brier_all)
    )
}

/// One grid run rendered: the folds, the test seasons and the pooled scores.
fn render_run(out: &mut String, r: &yaxel::Run, through_game: u32) {
    let early_cells = |e: Option<&yaxel::Scored>| {
        format!(
            "{} | {}",
            e.map_or("0".to_string(), |e| e.games.to_string()),
            e.map_or("– | –".to_string(), |e| format!(
                "{} | {}",
                signed(e.gain * 1000.0, 2),
                e.z.map_or("–".to_string(), |z| format!("{z:.1}"))
            ))
        )
    };
    out.push_str("### Folds\n\n| Fold | Chosen window | Prior minutes | Damping | Train gain | Test games | Brier base | Brier YAXEL | Test gain | z | Early games | Early gain | Early z |\n|---|---|---|---|---|---|---|---|---|---|---|---|---|\n");
    for (i, f) in r.folds.iter().enumerate() {
        out.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} | {} |\n",
            i + 1,
            f.chosen.window_days,
            f.chosen.prior_minutes,
            f.chosen.damping,
            signed(f.train.gain * 1000.0, 2),
            scored_cells(f.test.as_ref()),
            early_cells(f.test_early.as_ref())
        ));
    }
    out.push_str("\n### Seasons (each under its fold's parameters)\n\n| Season | Window | Prior minutes | Damping | Games | Brier base | Brier YAXEL | Gain | z | Early games | Early gain | Early z | New-player games | Gain | z |\n|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|\n");
    for l in &r.per_season {
        out.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} | {} |\n",
            l.season.label(),
            l.params.window_days,
            l.params.prior_minutes,
            l.params.damping,
            scored_cells(Some(&l.all)),
            early_cells(l.early.as_ref()),
            early_cells(l.new_floor.as_ref())
        ));
    }
    out.push_str("\n### Pooled test seasons\n\n| Cut | Games | Brier base | Brier YAXEL | Gain | z |\n|---|---|---|---|---|---|\n");
    for (label, scored) in [
        ("All games", r.pooled.as_ref()),
        (
            &*format!("Early season (through game {through_game})"),
            r.pooled_early.as_ref(),
        ),
        (
            "Games with a new player on a floor",
            r.pooled_new_floor.as_ref(),
        ),
        (
            "Early season, with a new player on a floor",
            r.pooled_new_floor_early.as_ref(),
        ),
    ] {
        out.push_str(&format!("| {label} | {} |\n", scored_cells(scored)));
    }
}

/// The backtest as a Markdown report: what was run; the new players' mark on Elo's own
/// forecasts, season by season; then the grid run with the term on every player and again
/// on the new players alone, each with its folds, its test seasons and the test seasons
/// pooled. The gain is in thousandths of Brier score per game, positive when YAXEL is the
/// closer.
pub fn render(b: &Backtest) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "# YAXEL backtest, {} to {}\n\n",
        b.from.label(),
        b.to.label()
    ));
    let Some(r) = &b.report else {
        out.push_str(&format!(
            "No report: {}\n",
            b.note.as_deref().unwrap_or("no seasons sampled")
        ));
        for s in &b.skipped {
            out.push_str(&format!("- {}: {}\n", s.season.label(), s.reason));
        }
        return out;
    };
    out.push_str(&format!(
        "Model {}. Seasons sampled: {}. Grid: windows {:?} days, prior minutes {:?}, dampings {:?}. Early season: neither team past game {}. New player: no record in a stored season before the one played, at least {} minutes in his team's previous game.\n\n",
        r.model,
        r.seasons.len(),
        r.grid.window_days,
        r.grid.prior_minutes,
        r.grid.dampings,
        r.through_game,
        r.new_players.minutes
    ));
    if !b.skipped.is_empty() {
        out.push_str("Skipped: ");
        out.push_str(
            &b.skipped
                .iter()
                .map(|s| format!("{} ({})", s.season.label(), s.reason))
                .collect::<Vec<_>>()
                .join(", "),
        );
        out.push_str(".\n\n");
    }
    let n = &r.new_players;
    out.push_str("## New players on Elo's own forecasts\n\nNo rule applied. Games with a new player on a floor, their share of the season, Elo's Brier score on them and on the rest; then the team-games with a new player on one floor only (a game with one on both says nothing about the bias), the bias (those teams' results less Elo's chance, in points of win chance, positive when they beat the rating), the same as Elo at even odds, and the bias in standard errors.\n\n");
    let cut_table = |out: &mut String, first: &str, rows: &[(String, u32, Option<&yaxel::Cut>)]| {
        out.push_str(&format!(
            "| {first} | Games {CUT_HEADER}\n|---|---{CUT_RULE}\n"
        ));
        for (label, games, c) in rows {
            out.push_str(&format!(
                "| {label} | {games} | {} |\n",
                c.map_or_else(|| NOT_TOLD.to_string(), cut_cells)
            ));
        }
    };
    cut_table(
        &mut out,
        "Season",
        &n.per_season
            .iter()
            .map(|l| (l.season.label(), l.games, l.all.as_ref()))
            .collect::<Vec<_>>(),
    );
    out.push_str(&format!(
        "\nEarly season (through game {}):\n\n",
        r.through_game
    ));
    cut_table(
        &mut out,
        "Season",
        &n.per_season
            .iter()
            .map(|l| (l.season.label(), l.early_games, l.early.as_ref()))
            .collect::<Vec<_>>(),
    );
    out.push_str("\nPooled over the seasons that can tell a new player:\n\n");
    cut_table(
        &mut out,
        "Cut",
        &[
            ("All games".to_string(), n.pooled.of, Some(&n.pooled)),
            (
                "Early season".to_string(),
                n.pooled_early.of,
                Some(&n.pooled_early),
            ),
        ],
    );
    if !n.not_told.is_empty() {
        out.push_str(&format!(
            "\nNew players cannot be told in {}: no earlier season of box scores among the ones the players are priced from, so every player drafted within five years, or without a draft year, would look new. No game of the season counts as one with a new player on a floor, and the season is left out of the pooled cut and of the run on the new players alone.\n",
            n.not_told
                .iter()
                .map(|s| s.label())
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    out.push_str("\n## New players against the absence-penalized rating\n\nThe same bias, read against the rating the live forecast prices at, with the box scores standing in for the injury report (the stored seasons before the feed have none): the full-strength rating, the season's games replayed with the players who sat each game taken out of the expectation, less the Elo of the players missing tonight. A player is on a team's roster from his first game for it, or from the season's start when he ended the season before with the team, to the day before his first game for another team, or the season's end, so a player out from the first game who returns is missing until he does and a player signed in season is not; a roster player with no line played sat the game, read from a box score with at least five players played; each is priced at the worth he entered the season with, as the floors are, at the expected penalty, a side capped at 400 Elo. Tonight's missing Elo is read two ways: from the team's previous game (the players on its roster at both games who sat it: the lineup is known before tip-off, the roster is read from the season's lines as a report would list it) and from tonight's box score (the players who sat tonight, which the report tells the live page before the game for the players it lists out). Missing Elo tonight is its mean over the team-games with a new player on the floor and over the team-games without one. A bias that closes against the penalized rating is the absence effect the live page already prices; one that stays is not.\n\n");
    let penalized_table =
        |out: &mut String, first: &str, rows: &[(String, u32, Option<&yaxel::Cut>)]| {
            out.push_str(&format!(
                "| {first} | Games {PENALIZED_HEADER}\n|---|---{PENALIZED_RULE}\n"
            ));
            for (label, games, c) in rows {
                out.push_str(&format!(
                    "| {label} | {games} | {} |\n",
                    c.map_or_else(|| PENALIZED_NOT_TOLD.to_string(), penalized_cells)
                ));
            }
        };
    penalized_table(
        &mut out,
        "Season",
        &n.per_season
            .iter()
            .map(|l| (l.season.label(), l.games, l.all.as_ref()))
            .collect::<Vec<_>>(),
    );
    out.push_str(&format!(
        "\nEarly season (through game {}):\n\n",
        r.through_game
    ));
    penalized_table(
        &mut out,
        "Season",
        &n.per_season
            .iter()
            .map(|l| (l.season.label(), l.early_games, l.early.as_ref()))
            .collect::<Vec<_>>(),
    );
    out.push_str("\nPooled over the seasons that can tell a new player:\n\n");
    penalized_table(
        &mut out,
        "Cut",
        &[
            ("All games".to_string(), n.pooled.of, Some(&n.pooled)),
            (
                "Early season".to_string(),
                n.pooled_early.of,
                Some(&n.pooled_early),
            ),
        ],
    );
    out.push_str(&format!(
        "\nBrier score of the three forecasts over all the games pooled, as a check that the penalized rating forecasts no worse. {} {}\n",
        brier_sentence("All games", &n.pooled),
        brier_sentence("Early season", &n.pooled_early)
    ));
    out.push_str("\n## The term on every player\n\n");
    render_run(&mut out, &r.all, r.through_game);
    out.push_str("\n## The term on the new players alone\n\n");
    match &r.new {
        Some(run) => render_run(&mut out, run, r.through_game),
        None => out.push_str(
            "Not run: fewer than two sampled seasons can tell a new player (an earlier season of box scores is needed).\n",
        ),
    }
    out.push_str("\nGain is base Brier less YAXEL Brier, in thousandths per game; z is the gain in standard errors over the games. A gain over two standard errors on the pooled test seasons, and on the early season, is the bar for taking a rule to the live page; for the term on the new players alone, the games with a new player on a floor are where it can act, and its gain there says whether it does. A bias of the new players' teams over two standard errors, with no gain from either term, says the rating is slow on them and the term as built is not the fix; the same bias gone against the absence-penalized rating says it is the absence effect the live page already prices.\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::models::{EloSource, ResultSource};

    fn game(id: i64, date: &str, home: i32, away: i32, score: (i32, i32)) -> Game {
        Game {
            id,
            season: SeasonYear::of(2027),
            game_date: NaiveDate::parse_from_str(date, "%Y-%m-%d").unwrap(),
            seq: 0,
            home_team_id: TeamId::new(home),
            away_team_id: TeamId::new(away),
            neutral: false,
            game_type: GameType::Regular,
            home_score: Some(score.0),
            away_score: Some(score.1),
            home_elo_pre: 1500.0,
            away_elo_pre: 1500.0,
            home_elo_post: 1500.0,
            away_elo_post: 1500.0,
            home_win_prob: None,
            elo_source: EloSource::Computed,
            result_source: ResultSource::Results,
            ref_home_elo_pre: None,
            ref_away_elo_pre: None,
            ref_home_elo_post: None,
            ref_away_elo_post: None,
            cup_stage: None,
        }
    }

    fn line(game_id: i64, player: i32, team: i32, seconds: i32, pts: i32) -> BoxLine {
        BoxLine {
            game_id,
            player_id: PlayerId::new(player),
            team_id: TeamId::new(team),
            seconds: Some(seconds),
            pts: Some(pts),
            reb: Some(5),
            oreb: Some(1),
            dreb: Some(4),
            ast: Some(3),
            stl: Some(1),
            blk: Some(1),
            tov: Some(2),
            pf: Some(2),
            fgm: Some(pts / 2),
            fga: Some(pts),
            fg3m: Some(0),
            ftm: Some(0),
            fta: Some(0),
        }
    }

    #[test]
    fn a_season_is_sampled_from_the_previous_games_floors_and_the_window_tables() {
        // Three games between teams 1 and 2 on three dates; players 11 and 12 for team 1
        // with four more (15 to 18), 21 and 22 for team 2 with three more (23 to 25), so
        // each box score has the five played lines that make it known whole; 12 sat out
        // the second game.
        let games = vec![
            game(1, "2026-10-22", 1, 2, (100, 90)),
            game(2, "2026-10-25", 2, 1, (95, 101)),
            game(3, "2026-11-02", 1, 2, (99, 100)),
        ];
        let mut lines = Vec::new();
        for (g, out) in [(1, false), (2, true), (3, false)] {
            lines.push(line(g, 11, 1, 2000, 20));
            lines.push(line(g, 12, 1, if out { 0 } else { 1800 }, 10));
            lines.push(line(g, 21, 2, 2100, 22));
            lines.push(line(g, 22, 2, 1700, 9));
            for p in 15..=18 {
                lines.push(line(g, p, 1, 1800, 5));
            }
            for p in 23..=25 {
                lines.push(line(g, p, 2, 1800, 5));
            }
        }
        let entering: HashMap<PlayerId, (f64, f64)> = [(11, 30.0), (12, 25.0), (21, 10.0)]
            .into_iter()
            .map(|(p, w)| (PlayerId::new(p), (w, 0.0)))
            .collect();
        let grid = Grid {
            window_days: vec![7, 28],
            prior_minutes: vec![250.0],
            dampings: vec![1.0],
        };
        let new_players: HashSet<PlayerId> = [PlayerId::new(22)].into_iter().collect();
        let s = build_season(
            SeasonYear::of(2027),
            41.0,
            &games,
            &lines,
            &HashMap::new(),
            &entering,
            &HashMap::new(),
            &new_players,
            None,
            &grid,
        );
        assert_eq!(
            (s.season, s.home_advantage, s.games.len()),
            (SeasonYear::of(2027), 41.0, 3)
        );
        // The first game has no floors: no previous game.
        let first = &s.games[0];
        assert_eq!((first.home_game_no, first.away_game_no), (1, 1));
        assert!(first
            .floors
            .iter()
            .all(|(h, a)| h.is_empty() && a.is_empty()));
        assert!(first.home_won);
        // The second game's floors are the first game's players, with their entering worths
        // (a player without one at 0) and, in both windows, their minutes from that game.
        let second = &s.games[1];
        assert_eq!((second.home_game_no, second.away_game_no), (2, 2));
        let (home, away) = &second.floors[0];
        assert_eq!(home.len(), 5, "team 2 is home: 21, 22 and three more");
        assert_eq!(away.len(), 6, "team 1: 11, 12 and four more");
        assert!(
            home.iter().any(|f| f.prior_elo == 10.0) && home.iter().any(|f| f.prior_elo == 0.0)
        );
        for f in home.iter().chain(away) {
            let (_, minutes) = f.recent.expect("the previous game is in the window");
            assert!(minutes > 25.0 && minutes < 36.0, "{minutes}");
            // The minutes of the previous game are the window's: one game played.
            assert!((f.last_minutes - minutes).abs() < 1e-9, "{f:?}");
        }
        // Player 22 is the new one, on team 2's floor; nobody else is.
        assert_eq!(home.iter().filter(|f| f.new).count(), 1);
        assert!(away.iter().all(|f| !f.new));
        assert!(!second.home_won);
        // The third game, eight days on: the 7-day window holds no game, the 28-day window
        // holds both; player 12 sat the second game, so he is off team 1's floor.
        let third = &s.games[2];
        let (home7, _) = &third.floors[0];
        assert!(home7.iter().all(|f| f.recent.is_none()));
        let (home28, away28) = &third.floors[1];
        assert_eq!(
            home28.len(),
            5,
            "team 1's floor is the five who played the second game"
        );
        assert!(home28.iter().all(|f| f.recent.is_some()));
        assert!(home28.iter().all(|f| f.prior_elo != 25.0), "12 is off it");
        assert_eq!(away28.len(), 5);
        assert_eq!((third.home_game_no, third.away_game_no), (3, 3));
        // The strengths: nobody sat the first game, so the second game's sides are whole
        // by their previous game; player 12 (worth 25) sat the second game, so team 1, the
        // away side, was missing 25 tonight, and before the third game it is missing 25 by
        // its previous game and whole tonight. Team 1 won the second game short-handed, so
        // its full-strength rating before the third game stands above the stored one and
        // team 2's below.
        assert_eq!(first.home_strength, Strength::plain(1500.0));
        assert_eq!(first.away_strength, Strength::plain(1500.0));
        assert_eq!(
            (
                second.away_strength.missing_last,
                second.away_strength.missing_tonight
            ),
            (0.0, 25.0)
        );
        assert_eq!(second.home_strength.missing_tonight, 0.0);
        assert_eq!(
            (
                third.home_strength.missing_last,
                third.home_strength.missing_tonight
            ),
            (25.0, 0.0)
        );
        assert!(third.home_strength.full_pre > 1500.0 && third.away_strength.full_pre < 1500.0);
    }
}
