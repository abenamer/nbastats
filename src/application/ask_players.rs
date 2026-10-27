//! The player side of the question box: the lookups behind the player, worth, availability
//! and forecast tools, implemented over the application's caches. Each returns compact JSON
//! for the model; a season without games or a team the store does not know comes back as
//! an `error` object the model can correct.

use chrono::NaiveDate;
use serde_json::{json, Value};

use crate::domain::cup;
use crate::domain::ids::{PlayerId, SeasonYear, TeamId};
use crate::domain::models::{Game, GameType};
use crate::domain::playoffs::season::{self as engine, SeasonForecast, SeasonParams};
use crate::domain::simulation::progress::Silent;
use crate::domain::worth;

use super::ask::PlayerLookups;
use super::boxscore;
use super::ports::{PlayerStore, StoreError};
use super::scheduler::current_season;
use super::state::{App, SeasonLoadError};
use super::worth as app_worth;

/// Players a name search returns to the model.
const MAX_PLAYERS: i64 = 8;
/// Injury-report lines returned without a team filter.
const MAX_REPORT_LINES: usize = 60;
/// Simulated seasons of the forecast comparison: a fifth of the forecast page's default,
/// enough for the expected-win figure; a title chance then carries about one point of
/// simulation noise. One comparison per question, see `ask::MAX_FORECAST_EFFECTS`.
const EFFECT_ITERATIONS: u64 = 2000;
/// Seasons `player_worth` returns without a season asked: the latest ones, so a long
/// career does not compute every season's tables on a cold cache.
const DEFAULT_WORTH_SEASONS: usize = 3;
/// Characters of an injury description passed to the model: the status and the return
/// date carry the pricing; the rest is colour.
const MAX_DESCRIPTION_CHARS: usize = 160;
const EFFECT_SEED: u64 = 7;

fn error(message: impl Into<String>) -> Value {
    json!({ "error": message.into() })
}

/// A season load that found no games is an answer for the model, not a failure.
fn or_error<T>(r: Result<T, SeasonLoadError>) -> Result<Result<T, Value>, StoreError> {
    match r {
        Ok(v) => Ok(Ok(v)),
        Err(SeasonLoadError::NoGames(s)) => Ok(Err(error(format!(
            "no games are stored for the {} season",
            s.label()
        )))),
        Err(SeasonLoadError::Store(e)) => Err(e),
    }
}

fn round(v: f64, digits: i32) -> f64 {
    let f = 10f64.powi(digits);
    // `+ 0.0` turns a negative zero (the sum of nothing) into a plain zero.
    (v * f).round() / f + 0.0
}

/// A team's rating at the latest played game of the season, or at the start of the season
/// when none is played, with the date it stands at.
pub fn rating_now(games: &[Game], team: TeamId) -> Option<(f64, NaiveDate, bool)> {
    let last_played = games
        .iter()
        .filter(|g| g.is_played() && (g.home_team_id == team || g.away_team_id == team))
        .max_by_key(|g| (g.game_date, g.seq));
    if let Some(g) = last_played {
        let elo = if g.home_team_id == team {
            g.home_elo_post
        } else {
            g.away_elo_post
        };
        return Some((elo, g.game_date, true));
    }
    let first = games
        .iter()
        .filter(|g| g.home_team_id == team || g.away_team_id == team)
        .min_by_key(|g| (g.game_date, g.seq))?;
    let elo = if first.home_team_id == team {
        first.home_elo_pre
    } else {
        first.away_elo_pre
    };
    Some((elo, first.game_date, false))
}

/// The penalties of a team's absences: today, and on average over the regular-season games
/// on the stored schedule it has not played yet, dated today or later (how many of them
/// there are): the games a forecast from today simulates. A game dated today without a
/// result is still to be played, whatever the hour.
pub fn penalty_summary(
    absences: &[worth::Absence],
    games: &[Game],
    team: TeamId,
    today: NaiveDate,
) -> (f64, f64, usize) {
    let today_penalty = worth::penalty(absences, team, today);
    let remaining: Vec<NaiveDate> = games
        .iter()
        .filter(|g| {
            g.game_type == GameType::Regular
                && !g.is_played()
                && g.game_date >= today
                && (g.home_team_id == team || g.away_team_id == team)
        })
        .map(|g| g.game_date)
        .collect();
    let mean = if remaining.is_empty() {
        0.0
    } else {
        remaining
            .iter()
            .map(|d| worth::penalty(absences, team, *d))
            .sum::<f64>()
            / remaining.len() as f64
    };
    (today_penalty, mean, remaining.len())
}

/// One team's figures from a forecast, the part the comparison reads.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EffectSide {
    pub expected_wins: f64,
    pub p_playoffs: f64,
    pub p_conf_finals: f64,
    pub p_finals: f64,
    pub p_champion: f64,
    pub mean_penalty: f64,
    pub remaining_games: i32,
}

impl EffectSide {
    fn of(f: &SeasonForecast, team: TeamId) -> Option<Self> {
        let t = f.teams.iter().find(|t| t.team_id == team)?;
        Some(Self {
            expected_wins: t.expected_wins,
            p_playoffs: t.p_playoffs,
            p_conf_finals: t.p_conf_finals,
            p_finals: t.p_finals,
            p_champion: t.p_champion,
            mean_penalty: t.absence_penalty,
            remaining_games: t.remaining_games,
        })
    }
}

/// The sides of the comparison and their differences; the chances are rounded to two
/// decimals, which is what 2,000 simulated seasons resolve. `held` is the run with this
/// team's absences of today held all season: the lower bound, with everyone healthy the
/// upper one and the dated forecast between them.
pub fn effect_json(with: EffectSide, healthy: EffectSide, held: Option<EffectSide>) -> Value {
    let row = |t: &EffectSide| {
        json!({
            "expected_wins": round(t.expected_wins, 1),
            "p_playoffs": round(t.p_playoffs, 2),
            "p_conference_finals": round(t.p_conf_finals, 2),
            "p_finals": round(t.p_finals, 2),
            "p_champion": round(t.p_champion, 2),
            "mean_penalty_elo": round(t.mean_penalty, 1),
        })
    };
    let mut v = json!({
        "with_report": row(&with),
        "everyone_healthy": row(&healthy),
        "healthy_minus_report": {
            "expected_wins": round(healthy.expected_wins - with.expected_wins, 1),
            "p_playoffs": round(healthy.p_playoffs - with.p_playoffs, 2),
            "p_champion": round(healthy.p_champion - with.p_champion, 2),
        },
        "team_games_remaining": with.remaining_games,
    });
    if let Some(h) = held {
        v["todays_absences_held_all_season"] = row(&h);
        v["range"] = json!({
            "expected_wins": [round(h.expected_wins, 1), round(healthy.expected_wins, 1)],
            "p_playoffs": [round(h.p_playoffs, 2), round(healthy.p_playoffs, 2)],
            "p_champion": [round(h.p_champion, 2), round(healthy.p_champion, 2)],
            "note": "the lower bound holds this team's absences covering its next game to the season's end, the upper has everyone healthy; the dated forecast (with_report) lies between them, up to about one point of simulation noise",
        });
    }
    v
}

/// The date of a team's next regular-season game without a result: what "tonight" means
/// to a forecast of a season in progress, and the first unplayed game of a season behind
/// us.
pub fn next_game_date(games: &[Game], team: TeamId) -> Option<NaiveDate> {
    games
        .iter()
        .filter(|g| g.game_type == GameType::Regular && !g.is_played() && g.involves(team))
        .map(|g| g.game_date)
        .min()
}

/// The forecast rows of one team under the runs, and their differences.
pub fn effect_rows(
    with: &SeasonForecast,
    without: &SeasonForecast,
    held: Option<&SeasonForecast>,
    team: TeamId,
) -> Option<Value> {
    let a = EffectSide::of(with, team)?;
    let b = EffectSide::of(without, team)?;
    let h = held.and_then(|f| EffectSide::of(f, team));
    let mut v = effect_json(a, b, h);
    v["absences_applied"] = json!(with.absences_applied);
    v["iterations"] = json!(with.iterations);
    v["as_of"] = json!(with.as_of);
    v["league_games_remaining"] = json!(with.games_remaining);
    Some(v)
}

impl App {
    async fn team_by_code(&self, code: &str) -> Result<Option<(TeamId, String)>, StoreError> {
        let teams = self.teams().await?;
        Ok(teams
            .values()
            .find(|t| t.code.eq_ignore_ascii_case(code))
            .map(|t| (t.id, t.code.clone())))
    }
}

impl PlayerLookups for App {
    async fn find_player(&self, query: &str) -> Result<Value, StoreError> {
        let players = self.store.search_players(query, MAX_PLAYERS).await?;
        let mut rows = Vec::with_capacity(players.len());
        for p in players {
            let seasons = self.store.player_seasons(p.id).await?;
            rows.push(json!({
                "player_id": p.id.as_i32(),
                "name": format!("{} {}", p.first_name, p.last_name).trim(),
                "position": p.position,
                "current_team": p.team_code,
                "active": p.active,
                "seasons": seasons.iter().map(|s| json!({
                    "season": s.season, "team": s.team_code, "games": s.games,
                    "playoff_games": s.playoff_games })).collect::<Vec<_>>(),
            }));
        }
        Ok(json!({ "query": query, "players": rows }))
    }

    async fn player_worth(
        &self,
        player_id: i32,
        season: Option<SeasonYear>,
    ) -> Result<Value, StoreError> {
        let id = PlayerId::new(player_id);
        let Some(player) = self.store.player(id).await? else {
            return Ok(error(format!("no player with identifier {player_id}")));
        };
        let teams = self.teams().await?;
        let code = |t: &TeamId| teams.get(t).map(|x| x.code.clone()).unwrap_or_default();
        let mut seasons: Vec<SeasonYear> = self
            .store
            .player_seasons(id)
            .await?
            .into_iter()
            .map(|s| s.season)
            .filter(|s| season.is_none_or(|want| want == *s))
            .collect();
        seasons.sort_unstable();
        seasons.dedup();
        // Without a season asked, the latest few: each season costs two tables on a cold
        // cache, and the model can ask for an earlier one by name.
        let mut other_seasons: Vec<SeasonYear> = Vec::new();
        if season.is_none() && seasons.len() > DEFAULT_WORTH_SEASONS {
            let keep = seasons.len() - DEFAULT_WORTH_SEASONS;
            other_seasons = seasons.drain(..keep).collect();
        }
        let mut rows = Vec::new();
        for s in seasons {
            let w = match or_error(app_worth::table(self, s).await)? {
                Ok(t) => t.players.iter().find(|w| w.player_id == id).map(|w| {
                    json!({
                        "worth_elo": round(w.worth_elo, 0), "se_elo": round(w.se_elo, 0),
                        "games_played": w.games_played, "games_missed": w.absences,
                        "teams": w.teams.iter().map(code).collect::<Vec<_>>(),
                    })
                }),
                Err(_) => None,
            };
            let b = match or_error(boxscore::table(self, s).await)? {
                Ok(t) if t.bpm_available => t
                    .players
                    .iter()
                    .filter(|p| p.player_id == id)
                    .map(|p| {
                        json!({
                            "team": code(&p.team_id), "games": p.games,
                            "minutes_share": round(p.minutes_share, 2),
                            "bpm": round(p.bpm, 1), "obpm": round(p.obpm, 1), "dbpm": round(p.dbpm, 1),
                            "vorp": round(p.vorp, 1), "per": p.per.map(|v| round(v, 1)),
                            "box_worth_elo": round(p.worth_elo, 0),
                        })
                    })
                    .collect::<Vec<_>>(),
                _ => vec![],
            };
            rows.push(json!({ "season": s, "from_missed_games": w, "from_box_score": b }));
        }
        Ok(json!({
            "player_id": player_id,
            "name": format!("{} {}", player.first_name, player.last_name).trim(),
            "current_team": player.team_code,
            "note": "worth_elo is what the team loses per game without him, in Elo (28 Elo is about one point of margin); se_elo is its standard error; box_worth_elo is the same from his box score, per game he plays",
            "seasons": rows,
            "other_seasons": other_seasons,
        }))
    }

    async fn season_worth_leaders(
        &self,
        season: SeasonYear,
        estimate: &str,
        limit: usize,
    ) -> Result<Value, StoreError> {
        if estimate == "box" {
            let (t, rows) = match or_error(boxscore::named_table(self, season).await)? {
                Ok(v) => v,
                Err(e) => return Ok(e),
            };
            if !t.bpm_available {
                return Ok(error(t.note.clone().unwrap_or_else(|| {
                    "no box-score worth for this season".to_string()
                })));
            }
            return Ok(json!({
                "season": season, "estimate": "box",
                "players": rows.iter().take(limit).map(|r| json!({
                    "player_id": r.row.player_id.as_i32(), "name": r.name, "team": r.team_code,
                    "bpm": round(r.row.bpm, 1), "vorp": round(r.row.vorp, 1),
                    "box_worth_elo": round(r.row.worth_elo, 0), "minutes_share": round(r.row.minutes_share, 2),
                })).collect::<Vec<_>>(),
            }));
        }
        let (_, rows) = match or_error(app_worth::named_table(self, season).await)? {
            Ok(v) => v,
            Err(e) => return Ok(e),
        };
        Ok(json!({
            "season": season, "estimate": "worth",
            "players": rows.iter().take(limit).map(|r| json!({
                "player_id": r.worth.player_id.as_i32(), "name": r.name, "teams": r.team_codes,
                "worth_elo": round(r.worth.worth_elo, 0), "se_elo": round(r.worth.se_elo, 0),
                "games_played": r.worth.games_played, "games_missed": r.worth.absences,
            })).collect::<Vec<_>>(),
        }))
    }

    async fn team_availability(
        &self,
        team: &str,
        season: Option<SeasonYear>,
    ) -> Result<Value, StoreError> {
        let today = self.today();
        let season = season.unwrap_or_else(|| current_season(today));
        let Some((team_id, code)) = self.team_by_code(team).await? else {
            return Ok(error(format!("unknown team code {team}")));
        };
        let data = match or_error(self.season_data(season).await)? {
            Ok(d) => d,
            Err(e) => return Ok(e),
        };
        let a = match or_error(app_worth::availability(self, season).await)? {
            Ok(a) => a,
            Err(e) => return Ok(e),
        };
        let lines: Vec<Value> = a
            .lines
            .iter()
            .filter(|l| l.team_id == team_id)
            .map(|l| {
                json!({
                    "player_id": l.player_id.as_i32(), "name": l.name, "status": l.status_text,
                    "return_date": l.return_date,
                    "worth_elo": l.worth_elo.map(|v| round(v, 0)), "se_elo": l.se_elo.map(|v| round(v, 0)),
                    "worth_season": l.worth_season, "worth_source": l.worth_source,
                    "penalty_elo": l.penalty_elo.map(|v| round(v, 0)), "until": l.until,
                    "weight": l.weight.map(|v| round(v, 2)), "weight_set_by": l.weight_source,
                    "until_set_by": l.span, "games_missed_so_far": l.missed_so_far,
                    "box_scores_through": l.lines_through, "on_report": l.listed,
                })
            })
            .collect();
        let unlisted = a
            .lines
            .iter()
            .filter(|l| l.team_id == team_id && !l.listed)
            .count();
        let (today_penalty, mean_penalty, remaining) =
            penalty_summary(&a.absences, &data.games, team_id, today);
        let on_schedule = data
            .games
            .iter()
            .filter(|g| g.game_type == GameType::Regular && g.involves(team_id))
            .count();
        let Some(rating) = rating_now(&data.games, team_id) else {
            return Ok(error(format!(
                "{code} is not in the {} season",
                season.label()
            )));
        };
        let rating = Some(rating);
        Ok(json!({
            "team": code, "season": season, "report_date": a.report_date, "today": today,
            "rating_elo": rating.map(|(e, _, _)| round(e, 0)),
            "rating_date": rating.map(|(_, d, _)| d),
            "rating_is_preseason": rating.map(|(_, _, played)| !played),
            "penalty_today_elo": round(today_penalty, 0),
            "rating_with_absences_today": rating.map(|(e, _, _)| round(e - today_penalty, 0)),
            "everyone_healthy_gain_elo": round(today_penalty, 0),
            "mean_penalty_remaining_games_elo": round(mean_penalty, 0),
            "remaining_games_counted": "regular-season games on the stored schedule with no result, dated today or later: the games a forecast from today simulates",
            "remaining_regular_season_games": remaining,
            "regular_season_games_on_schedule": on_schedule,
            "regular_season_games_played": data.games.iter().filter(|g| g.game_type == GameType::Regular && g.is_played() && g.involves(team_id)).count(),
            "schedule_note": cup::schedule_note(season, on_schedule),
            "lines": lines,
            "absent_without_a_report_line": unlisted,
            "mean_run_of_missed_games": a.survival.as_ref().map(|s| round(s.mean_run_games, 1)),
            "what_a_status_has_meant": a.statuses.as_ref().map(|t| t.curves.iter().map(|c| json!({
                "status": c.status, "already_in_a_run": c.in_run, "lines_followed": c.samples,
                "share_missing_the_next_game": round(c.weight, 2), "games_missed_of_the_next_30": round(c.expected_missed, 1),
                "span_games": round(c.span_games, 1),
            })).collect::<Vec<_>>()),
            "note": a.note.unwrap_or_else(|| "A penalty is what the team plays without on a game date; the rating itself does not change. Lines without a penalty have returned, have no worth estimate, or have a worth at or below zero. A line with on_report false is a regular player absent from his team's last games who is not on the report. weight is the share of the worth taken off: measured when the stored reports show how often a line of this status, for a player not yet out or already in a run, missed the next game; else the status default (out 1, doubtful 0.75, questionable or day-to-day 0.5, probable 0.25). until_set_by says what ended the span: the report's return date when it lies beyond the next game, else the expected remaining games of a run this long (expected), the games a line of this status has gone on to miss (measured), the status default, or the season.".to_string()),
        }))
    }

    async fn forecast_effect(
        &self,
        team: &str,
        season: Option<SeasonYear>,
    ) -> Result<Value, StoreError> {
        let today = self.today();
        let season = season.unwrap_or_else(|| current_season(today));
        let Some((team_id, code)) = self.team_by_code(team).await? else {
            return Ok(error(format!("unknown team code {team}")));
        };
        let data = match or_error(self.season_data(season).await)? {
            Ok(d) => d,
            Err(e) => return Ok(e),
        };
        let a = match or_error(app_worth::availability(self, season).await)? {
            Ok(a) => a,
            Err(e) => return Ok(e),
        };
        let home = self.home_advantage(season).await?;
        let teams = self.teams().await?;
        let mut params = SeasonParams {
            hold_absences: false,
            season,
            as_of: Some(today),
            iterations: EFFECT_ITERATIONS,
            seed: EFFECT_SEED,
            home_advantage: 100.0,
            regular_home_advantage: Some(home.regular),
            regular_season_k: Some(20.0),
            playoff_k: None,
            playoff_scale: 1.0,
            absences: a.absences.clone(),
        };
        app_worth::fix_penalties(&mut params.absences);
        // "Everyone healthy" means this team's players back; the other teams keep their
        // absences as reported. "Held" means this team's absences of today kept to the
        // season's end, the other teams' as dated. With nothing priced for this team the
        // runs would be identical: run once.
        let nothing_priced = !params.absences.iter().any(|a| a.team_id == team_id);
        let games = data.games.clone();
        let teams_for_run = teams.clone();
        let outcome = tokio::task::spawn_blocking(move || {
            let with = engine::run(&params, &games, &teams_for_run, &Silent)?;
            if nothing_priced {
                return Ok::<_, engine::SeasonError>((with.clone(), with, None));
            }
            let mut healthy = params.clone();
            healthy.absences.retain(|a| a.team_id != team_id);
            let without = engine::run(&healthy, &games, &teams_for_run, &Silent)?;
            // "Today's absences" are the ones covering the team's next game to simulate:
            // tonight in the season in progress, and still defined for a season behind us.
            // Only this team's absences are held; the others keep their dated spans.
            let anchor = next_game_date(&games, team_id).unwrap_or(with.as_of);
            let mut held_params = params.clone();
            let mut extended = 0;
            if let Some(last) = engine::last_regular_game(&games) {
                for a in &mut held_params.absences {
                    if a.team_id == team_id && a.covers(anchor) && a.to < last {
                        a.to = last;
                        extended += 1;
                    }
                }
            }
            let held = if extended > 0 {
                Some(engine::run(&held_params, &games, &teams_for_run, &Silent)?)
            } else {
                None
            };
            Ok((with, without, held))
        })
        .await
        .map_err(|e| StoreError::Query(e.to_string()))?;
        let (with, without, held) = match outcome {
            Ok(v) => v,
            Err(e) => return Ok(error(e.to_string())),
        };
        let Some(rows) = effect_rows(&with, &without, held.as_ref(), team_id) else {
            return Ok(error(format!(
                "{code} is not in the {} season",
                season.label()
            )));
        };
        let mut out = rows;
        out["team"] = json!(code);
        out["season"] = json!(season);
        out["note"] = json!(if nothing_priced {
            "No absence is priced for this team (no report, a report outside the season, or no listed player with a worth), so both sides are the same forecast."
        } else {
            "Runs of the season engine from today with the same seed: with the injury report's absences priced at their estimates (with_report), with this team's absences removed while the other teams keep theirs (everyone_healthy), and, when one of this team's absences covers its next game and ends before the last game, with those held to the season's end (todays_absences_held_all_season). The last two are the bounds; the dated forecast lies between them up to about one point of simulation noise, where exactly depending on when the players return. Only the regular-season games the spans cover take the penalty; the play-in and playoffs run at full ratings."
        });
        if let Some(n) = a.note {
            out["report_note"] = json!(n);
        }
        Ok(out)
    }

    async fn injury_report(
        &self,
        date: Option<NaiveDate>,
        team: Option<&str>,
    ) -> Result<Value, StoreError> {
        let date = match date {
            Some(d) => d,
            None => match self.store.latest_injury_report_date().await? {
                Some(d) => d,
                None => return Ok(error("no injury report is stored")),
            },
        };
        let lines = self.store.injury_report(date).await?;
        let team = team.map(|t| t.to_ascii_uppercase());
        let rows: Vec<Value> = lines
            .iter()
            .filter(|l| {
                team.as_ref()
                    .is_none_or(|t| l.team_code.eq_ignore_ascii_case(t))
            })
            .take(MAX_REPORT_LINES)
            .map(|l| {
                json!({
                    "player_id": l.player_id.as_i32(), "name": l.player_name, "team": l.team_code,
                    "status": l.status, "return_date": l.return_date,
                    "description": l.description.as_deref().map(|d| d.chars().take(MAX_DESCRIPTION_CHARS).collect::<String>()),
                })
            })
            .collect();
        Ok(json!({
            "report_date": date,
            "report_lines_total": lines.len(),
            "lines_returned": rows.len(),
            "lines": rows,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::models::{EloSource, ResultSource};

    fn game(id: i64, day: u32, home: i32, away: i32, played: bool) -> Game {
        Game {
            id,
            season: SeasonYear::of(2027),
            game_date: NaiveDate::from_ymd_opt(2026, 10, day).unwrap(),
            seq: 0,
            home_team_id: TeamId::new(home),
            away_team_id: TeamId::new(away),
            neutral: false,
            game_type: GameType::Regular,
            home_score: played.then_some(100),
            away_score: played.then_some(90),
            home_elo_pre: 1600.0,
            away_elo_pre: 1500.0,
            home_elo_post: 1604.0,
            away_elo_post: 1496.0,
            home_win_prob: None,
            elo_source: EloSource::Computed,
            result_source: ResultSource::Results,
            ref_home_elo_pre: None,
            ref_away_elo_pre: None,
            ref_home_elo_post: None,
            ref_away_elo_post: None,
        }
    }

    #[test]
    fn rounding_never_prints_a_negative_zero() {
        let none: f64 = std::iter::empty::<f64>().sum();
        assert!(round(none, 0).is_sign_positive());
        assert_eq!(round(23.94, 1), 23.9);
        assert_eq!(round(-0.04, 1), 0.0);
        assert!(round(-0.04, 1).is_sign_positive());
    }

    #[test]
    fn the_comparison_reports_both_sides_and_the_healthy_minus_report_differences() {
        let with = EffectSide {
            expected_wins: 50.2,
            p_playoffs: 0.9,
            p_conf_finals: 0.3,
            p_finals: 0.15,
            p_champion: 0.081,
            mean_penalty: 31.4,
            remaining_games: 60,
        };
        let healthy = EffectSide {
            expected_wins: 52.6,
            p_playoffs: 0.95,
            p_conf_finals: 0.36,
            p_finals: 0.19,
            p_champion: 0.112,
            mean_penalty: 0.0,
            remaining_games: 60,
        };
        let held = EffectSide {
            expected_wins: 44.0,
            p_playoffs: 0.6,
            p_conf_finals: 0.1,
            p_finals: 0.04,
            p_champion: 0.012,
            mean_penalty: 120.0,
            remaining_games: 60,
        };
        let v = effect_json(with, healthy, Some(held));
        assert_eq!(v["healthy_minus_report"]["expected_wins"], 2.4);
        assert_eq!(v["healthy_minus_report"]["p_champion"], 0.03);
        assert_eq!(v["with_report"]["mean_penalty_elo"], 31.4);
        assert_eq!(v["everyone_healthy"]["p_champion"], 0.11);
        assert_eq!(v["team_games_remaining"], 60);
        assert_eq!(v["todays_absences_held_all_season"]["expected_wins"], 44.0);
        assert_eq!(v["range"]["expected_wins"], json!([44.0, 52.6]));
        // Without a held run, no bounds.
        let v = effect_json(with, healthy, None);
        assert!(v.get("range").is_none());
    }

    #[test]
    fn the_next_game_is_the_first_unplayed_regular_season_game_of_the_team() {
        let games = vec![
            game(1, 20, 1, 2, true),
            game(2, 25, 1, 2, false),
            game(3, 22, 2, 3, false),
        ];
        assert_eq!(
            next_game_date(&games, TeamId::new(1)),
            NaiveDate::from_ymd_opt(2026, 10, 25)
        );
        assert_eq!(
            next_game_date(&games, TeamId::new(2)),
            NaiveDate::from_ymd_opt(2026, 10, 22)
        );
        assert_eq!(next_game_date(&games, TeamId::new(9)), None);
    }

    #[test]
    fn the_rating_now_is_the_latest_played_games_post_rating_or_the_start() {
        let games = vec![
            game(1, 20, 1, 2, true),
            game(2, 22, 2, 1, true),
            game(3, 25, 1, 2, false),
        ];
        // Team 1 was away in the latest played game.
        assert_eq!(
            rating_now(&games, TeamId::new(1)),
            Some((1496.0, NaiveDate::from_ymd_opt(2026, 10, 22).unwrap(), true))
        );
        let unplayed = vec![game(3, 25, 1, 2, false)];
        assert_eq!(
            rating_now(&unplayed, TeamId::new(2)),
            Some((
                1500.0,
                NaiveDate::from_ymd_opt(2026, 10, 25).unwrap(),
                false
            ))
        );
        assert_eq!(rating_now(&unplayed, TeamId::new(9)), None);
    }

    #[test]
    fn the_penalty_summary_counts_today_and_the_remaining_games() {
        let d = |day: u32| NaiveDate::from_ymd_opt(2026, 10, day).unwrap();
        let games = vec![
            game(1, 20, 1, 2, true),
            game(2, 24, 1, 2, false),
            game(3, 26, 2, 1, false),
            game(4, 30, 1, 2, false),
        ];
        let absences = vec![worth::Absence {
            team_id: TeamId::new(1),
            from: d(23),
            to: d(27),
            elo: 60.0,
            player_id: None,
            se_elo: 0.0,
        }];
        let (today, mean, n) = penalty_summary(&absences, &games, TeamId::new(1), d(24));
        // The unplayed game dated today is still to be played, so it counts with the two
        // later ones; two of the three fall in the span.
        assert_eq!((today, n), (60.0, 3));
        assert!((mean - 40.0).abs() < 1e-9);
        let (today, mean, _) = penalty_summary(&absences, &games, TeamId::new(2), d(24));
        assert_eq!((today, mean), (0.0, 0.0));
    }
}
