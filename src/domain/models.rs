//! Domain types shared by the database layer, the engines and the web layer.
//!
//! Every lifecycle that the database stores as `TEXT` is modelled as an enum here, with the
//! legal transitions encoded as methods: the compiler, not a `CHECK` constraint alone, keeps
//! a job from going `completed -> running` or a playoff team from "winning" the first round
//! after losing the Finals.

use std::fmt;
use std::str::FromStr;

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

// ---------------------------------------------------------------------------------------
// Enumerated states
// ---------------------------------------------------------------------------------------

/// Implemented for every string-backed enum so the web layer and SQL can round-trip them.
macro_rules! text_enum {
    ($(#[$meta:meta])* $name:ident { $($variant:ident => $text:literal),+ $(,)? }) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
        #[serde(rename_all = "snake_case")]
        $(#[$meta])*
        pub enum $name { $($variant),+ }

        impl $name {
            pub const ALL: &'static [$name] = &[$($name::$variant),+];
            pub const fn as_str(self) -> &'static str {
                match self { $($name::$variant => $text),+ }
            }
        }

        // Stored as plain TEXT: delegate type info, encoding and decoding to `&str`, so the
        // enum binds in scalar and array positions without a PostgreSQL enum type.
        impl sqlx::Type<sqlx::Postgres> for $name {
            fn type_info() -> sqlx::postgres::PgTypeInfo { <&str as sqlx::Type<sqlx::Postgres>>::type_info() }
            fn compatible(ty: &sqlx::postgres::PgTypeInfo) -> bool { <&str as sqlx::Type<sqlx::Postgres>>::compatible(ty) }
        }
        impl sqlx::postgres::PgHasArrayType for $name {
            fn array_type_info() -> sqlx::postgres::PgTypeInfo { <&str as sqlx::postgres::PgHasArrayType>::array_type_info() }
        }
        impl<'q> sqlx::Encode<'q, sqlx::Postgres> for $name {
            fn encode_by_ref(&self, buf: &mut sqlx::postgres::PgArgumentBuffer) -> Result<sqlx::encode::IsNull, sqlx::error::BoxDynError> {
                <&str as sqlx::Encode<'q, sqlx::Postgres>>::encode_by_ref(&self.as_str(), buf)
            }
        }
        impl<'r> sqlx::Decode<'r, sqlx::Postgres> for $name {
            fn decode(value: sqlx::postgres::PgValueRef<'r>) -> Result<Self, sqlx::error::BoxDynError> {
                let s = <&str as sqlx::Decode<'r, sqlx::Postgres>>::decode(value)?;
                Ok(s.parse::<$name>()?)
            }
        }
        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { f.write_str(self.as_str()) }
        }
        impl FromStr for $name {
            type Err = UnknownVariant;
            fn from_str(s: &str) -> Result<Self, Self::Err> {
                match s { $($text => Ok($name::$variant),)+ other => Err(UnknownVariant(stringify!($name), other.to_string())) }
            }
        }
    };
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("unknown {0} variant: {1:?}")]
pub struct UnknownVariant(pub &'static str, pub String);

text_enum! {
    /// Classification of a game within a season. Only `Regular` games count in the standings;
    /// `CupFinal` is the NBA Cup (in-season tournament) final, an extra game since 2023-24.
    GameType { Regular => "regular", PlayIn => "play_in", CupFinal => "cup_final", Playoff => "playoff" }
}

text_enum! {
    Conference { East => "East", West => "West" }
}

impl Conference {
    pub const fn index(self) -> usize {
        match self {
            Conference::East => 0,
            Conference::West => 1,
        }
    }
    pub const fn from_index(i: usize) -> Conference {
        if i == 0 {
            Conference::East
        } else {
            Conference::West
        }
    }
}

text_enum! {
    /// How far a team got in a postseason. Variants are ordered from worst to best, so
    /// `PartialOrd` answers "did the team reach at least round X".
    #[derive(PartialOrd, Ord)]
    PlayoffResult {
        Missed => "missed",
        PlayIn => "play_in",
        FirstRound => "first_round",
        SecondRound => "second_round",
        ConfFinals => "conf_finals",
        Finals => "finals",
        Champion => "champion",
    }
}

/// A playoff round. `Round` is the state of a team's progression through the bracket.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Round {
    First,
    Second,
    ConfFinals,
    Finals,
}

impl Round {
    pub const ALL: [Round; 4] = [Round::First, Round::Second, Round::ConfFinals, Round::Finals];

    /// 1-based round number as used in bracket arithmetic.
    pub const fn number(self) -> u8 {
        match self {
            Round::First => 1,
            Round::Second => 2,
            Round::ConfFinals => 3,
            Round::Finals => 4,
        }
    }
    pub const fn from_number(n: u8) -> Option<Round> {
        match n {
            1 => Some(Round::First),
            2 => Some(Round::Second),
            3 => Some(Round::ConfFinals),
            4 => Some(Round::Finals),
            _ => None,
        }
    }
    pub const fn next(self) -> Option<Round> {
        match self {
            Round::First => Some(Round::Second),
            Round::Second => Some(Round::ConfFinals),
            Round::ConfFinals => Some(Round::Finals),
            Round::Finals => None,
        }
    }
    /// Result label for a team eliminated in this round.
    pub const fn eliminated(self) -> PlayoffResult {
        match self {
            Round::First => PlayoffResult::FirstRound,
            Round::Second => PlayoffResult::SecondRound,
            Round::ConfFinals => PlayoffResult::ConfFinals,
            Round::Finals => PlayoffResult::Finals,
        }
    }
    /// Result label for a team that wins this round.
    pub const fn won(self) -> PlayoffResult {
        match self.next() {
            Some(r) => r.eliminated(),
            None => PlayoffResult::Champion,
        }
    }
    /// Round at `depth` series before the Finals (0 = Finals).
    pub const fn from_depth(depth: u8) -> Round {
        match depth {
            0 => Round::Finals,
            1 => Round::ConfFinals,
            2 => Round::Second,
            _ => Round::First,
        }
    }
}

text_enum! {
    JobKind { Alpha => "alpha", Playoff => "playoff", Season => "season", Backtest => "backtest", Dynasty => "dynasty" }
}

text_enum! {
    /// Lifecycle of a simulation job.
    JobStatus {
        Queued => "queued",
        Running => "running",
        Completed => "completed",
        Failed => "failed",
        Interrupted => "interrupted",
        Cancelled => "cancelled",
    }
}

/// Events that move a job through its lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobEvent {
    Claim,
    Complete,
    Fail,
    /// The process hosting the job went away (restart, SIGTERM).
    Interrupt,
    Cancel,
    Retry,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("job cannot go from {from} on {event:?}")]
pub struct InvalidTransition {
    pub from: JobStatus,
    pub event: JobEvent,
}

impl JobStatus {
    /// The job state machine. Terminal states can only be re-queued through `Retry`.
    pub const fn transition(self, event: JobEvent) -> Result<JobStatus, InvalidTransition> {
        use JobEvent as E;
        use JobStatus as S;
        match (self, event) {
            (S::Queued, E::Claim) => Ok(S::Running),
            (S::Queued, E::Cancel) => Ok(S::Cancelled),
            (S::Running, E::Complete) => Ok(S::Completed),
            (S::Running, E::Fail) => Ok(S::Failed),
            (S::Running, E::Interrupt) => Ok(S::Interrupted),
            (S::Running, E::Cancel) => Ok(S::Cancelled),
            (S::Failed | S::Interrupted | S::Cancelled, E::Retry) => Ok(S::Queued),
            (from, event) => Err(InvalidTransition { from, event }),
        }
    }
    pub const fn is_terminal(self) -> bool {
        matches!(self, JobStatus::Completed | JobStatus::Failed | JobStatus::Interrupted | JobStatus::Cancelled)
    }
    pub const fn can_retry(self) -> bool {
        self.transition(JobEvent::Retry).is_ok()
    }
}

text_enum! {
    ImportStatus { Running => "running", Completed => "completed", Failed => "failed" }
}

// ---------------------------------------------------------------------------------------
// Rows
// ---------------------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Franchise {
    pub id: i32,
    pub key: String,
    pub name: String,
    pub active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Team {
    pub id: i32,
    pub code: String,
    pub franchise_id: i32,
    pub name: String,
    pub first_season: Option<i32>,
    pub last_season: Option<i32>,
}

impl Team {
    pub fn franchise_key(&self) -> &str {
        crate::domain::reference::lookup_team(&self.code).map(|t| t.1).unwrap_or(&self.code)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Season {
    pub season: i32,
    pub label: String,
    pub team_count: i32,
    pub game_count: i32,
    pub playoff_game_count: i32,
    pub play_in_game_count: i32,
    pub cup_final_game_count: i32,
    pub regular_season_end: Option<NaiveDate>,
    pub playoffs_start: Option<NaiveDate>,
    pub playoffs_end: Option<NaiveDate>,
    pub complete: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Game {
    pub id: i64,
    pub season: i32,
    pub game_date: NaiveDate,
    pub seq: i16,
    pub home_team_id: i32,
    pub away_team_id: i32,
    pub neutral: bool,
    pub game_type: GameType,
    pub home_score: Option<i32>,
    pub away_score: Option<i32>,
    pub home_elo_pre: f64,
    pub away_elo_pre: f64,
    pub home_elo_post: f64,
    pub away_elo_post: f64,
    pub home_win_prob: Option<f64>,
}

/// Outcome of a played game from the home team's perspective.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    HomeWin { margin: i32 },
    AwayWin { margin: i32 },
}

impl Outcome {
    pub const fn home_won(self) -> bool {
        matches!(self, Outcome::HomeWin { .. })
    }
    pub const fn margin(self) -> i32 {
        match self {
            Outcome::HomeWin { margin } | Outcome::AwayWin { margin } => margin,
        }
    }
}

impl Game {
    /// `None` for a scheduled game that has not been played.
    pub fn outcome(&self) -> Option<Outcome> {
        match (self.home_score, self.away_score) {
            (Some(h), Some(a)) if h > a => Some(Outcome::HomeWin { margin: h - a }),
            (Some(h), Some(a)) => Some(Outcome::AwayWin { margin: a - h }),
            _ => None,
        }
    }
    pub fn home_won(&self) -> Option<bool> {
        self.outcome().map(Outcome::home_won)
    }
    pub fn is_playoff(&self) -> bool {
        self.game_type == GameType::Playoff
    }
    pub fn is_played(&self) -> bool {
        self.outcome().is_some()
    }
    pub fn involves(&self, team_id: i32) -> bool {
        self.home_team_id == team_id || self.away_team_id == team_id
    }
    /// (elo_pre, elo_post) of `team_id` in this game.
    pub fn elo_of(&self, team_id: i32) -> (f64, f64) {
        if self.home_team_id == team_id {
            (self.home_elo_pre, self.home_elo_post)
        } else {
            (self.away_elo_pre, self.away_elo_post)
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct TeamSeason {
    pub season: i32,
    pub team_id: i32,
    pub franchise_id: i32,
    pub conference: Option<Conference>,
    pub division: Option<String>,
    pub wins: i32,
    pub losses: i32,
    pub win_pct: f64,
    pub elo_start: Option<f64>,
    pub elo_regular_end: Option<f64>,
    pub elo_pre_playoffs: Option<f64>,
    pub elo_end: Option<f64>,
    pub elo_peak: Option<f64>,
    pub made_play_in: bool,
    pub made_playoffs: bool,
    pub seed: Option<i32>,
    pub playoff_result: PlayoffResult,
}

/// Team-season joined with team and franchise labels (for display).
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct TeamSeasonView {
    pub season: i32,
    pub team_id: i32,
    pub code: String,
    pub team_name: String,
    pub franchise_id: i32,
    pub franchise_key: String,
    pub franchise_name: String,
    pub conference: Option<Conference>,
    pub division: Option<String>,
    pub wins: i32,
    pub losses: i32,
    pub win_pct: f64,
    pub elo_start: Option<f64>,
    pub elo_regular_end: Option<f64>,
    pub elo_pre_playoffs: Option<f64>,
    pub elo_end: Option<f64>,
    pub elo_peak: Option<f64>,
    pub made_play_in: bool,
    pub made_playoffs: bool,
    pub seed: Option<i32>,
    pub playoff_result: PlayoffResult,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ChampionshipView {
    pub season: i32,
    pub champion_code: String,
    pub champion_name: String,
    pub franchise_key: String,
    pub franchise_name: String,
    pub runner_up_code: Option<String>,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct DataImport {
    pub id: i64,
    pub source: String,
    pub source_url: Option<String>,
    pub file_name: Option<String>,
    pub file_sha256: Option<String>,
    pub format: String,
    pub status: ImportStatus,
    pub rows_read: i64,
    pub games_inserted: i64,
    pub games_updated: i64,
    pub games_unchanged: i64,
    pub rows_rejected: i64,
    pub min_season: Option<i32>,
    pub max_season: Option<i32>,
    pub error: Option<String>,
    pub notes: Option<String>,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct SimulationJob {
    pub id: Uuid,
    pub kind: JobKind,
    pub name: Option<String>,
    pub status: JobStatus,
    pub params: serde_json::Value,
    pub result: Option<serde_json::Value>,
    pub progress: f64,
    pub error: Option<String>,
    pub seed: Option<i64>,
    pub app_version: String,
    pub model_version: String,
    pub attempts: i32,
    pub created_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
    pub updated_at: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn job_state_machine() {
        use JobEvent::*;
        use JobStatus::*;
        assert_eq!(Queued.transition(Claim), Ok(Running));
        assert_eq!(Running.transition(Complete), Ok(Completed));
        assert_eq!(Running.transition(Interrupt), Ok(Interrupted));
        assert_eq!(Interrupted.transition(Retry), Ok(Queued));
        assert!(Completed.transition(Retry).is_err());
        assert!(Completed.transition(Claim).is_err());
        assert!(Queued.transition(Complete).is_err());
        assert!(Failed.can_retry());
        assert!(!Completed.can_retry());
    }

    #[test]
    fn text_round_trip() {
        for s in JobStatus::ALL {
            assert_eq!(s.as_str().parse::<JobStatus>().unwrap(), *s);
        }
        assert_eq!("play_in".parse::<GameType>().unwrap(), GameType::PlayIn);
        assert!("bogus".parse::<GameType>().is_err());
        assert_eq!(serde_json::to_string(&PlayoffResult::ConfFinals).unwrap(), "\"conf_finals\"");
    }

    #[test]
    fn playoff_results_are_ordered() {
        assert!(PlayoffResult::Champion > PlayoffResult::Finals);
        assert!(PlayoffResult::FirstRound > PlayoffResult::Missed);
        assert_eq!(Round::First.won(), PlayoffResult::SecondRound);
        assert_eq!(Round::Finals.won(), PlayoffResult::Champion);
        assert_eq!(Round::from_depth(1), Round::ConfFinals);
    }

    #[test]
    fn outcome_from_scores() {
        let g = Game {
            id: 1, season: 2016, game_date: NaiveDate::from_ymd_opt(2016, 6, 19).unwrap(), seq: 0, home_team_id: 1,
            away_team_id: 2, neutral: false, game_type: GameType::Playoff, home_score: Some(89), away_score: Some(93),
            home_elo_pre: 1764.8, away_elo_pre: 1749.8, home_elo_post: 1755.6, away_elo_post: 1759.0, home_win_prob: Some(0.66),
        };
        assert_eq!(g.outcome(), Some(Outcome::AwayWin { margin: 4 }));
        assert_eq!(g.elo_of(2), (1749.8, 1759.0));
    }
}
