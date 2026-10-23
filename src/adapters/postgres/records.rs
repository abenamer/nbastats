//! Row mapping for the PostgreSQL adapter (REAS-DATA-001).
//!
//! Domain types carry no SQLx derives. This module implements `sqlx::Type`, `Encode` and
//! `Decode` for the identifier newtypes and the text-backed enums, and `FromRow` for the
//! domain structs, so queries can still use `query_as::<_, Team>`.

use chrono::{DateTime, NaiveDate, Utc};
use sqlx::postgres::{PgArgumentBuffer, PgHasArrayType, PgRow, PgTypeInfo, PgValueRef};
use sqlx::{Decode, Encode, FromRow, Postgres, Row, Type};
use uuid::Uuid;

use crate::domain::ids::{FranchiseId, SeasonYear, TeamId};
use crate::domain::models::{
    ChampionshipView, Conference, DataImport, EloSource, Franchise, Game, GameType, ImportStatus,
    JobKind, JobStatus, PlayoffResult, ResultSource, Season, SimulationJob, Team, TeamSeason,
    TeamSeasonView,
};
use crate::domain::records::TeamSeasonStat;

// ---------------------------------------------------------------------------------------
// Identifier newtypes: stored as INTEGER
// ---------------------------------------------------------------------------------------

macro_rules! int_newtype {
    ($name:ident, $new:expr, $raw:expr) => {
        impl Type<Postgres> for $name {
            fn type_info() -> PgTypeInfo {
                <i32 as Type<Postgres>>::type_info()
            }
            fn compatible(ty: &PgTypeInfo) -> bool {
                <i32 as Type<Postgres>>::compatible(ty)
            }
        }
        impl PgHasArrayType for $name {
            fn array_type_info() -> PgTypeInfo {
                <i32 as PgHasArrayType>::array_type_info()
            }
        }
        impl<'q> Encode<'q, Postgres> for $name {
            fn encode_by_ref(
                &self,
                buf: &mut PgArgumentBuffer,
            ) -> Result<sqlx::encode::IsNull, sqlx::error::BoxDynError> {
                <i32 as Encode<'q, Postgres>>::encode_by_ref(&($raw)(*self), buf)
            }
        }
        impl<'r> Decode<'r, Postgres> for $name {
            fn decode(value: PgValueRef<'r>) -> Result<Self, sqlx::error::BoxDynError> {
                let raw = <i32 as Decode<'r, Postgres>>::decode(value)?;
                ($new)(raw)
            }
        }
    };
}

int_newtype!(TeamId, |raw| Ok(TeamId::new(raw)), |id: TeamId| id.as_i32());
int_newtype!(
    FranchiseId,
    |raw| Ok(FranchiseId::new(raw)),
    |id: FranchiseId| id.as_i32()
);
int_newtype!(
    SeasonYear,
    |raw| SeasonYear::try_new(raw).map_err(|e| Box::new(e) as sqlx::error::BoxDynError),
    |s: SeasonYear| s.end_year()
);

// ---------------------------------------------------------------------------------------
// Text-backed enums: stored as TEXT
// ---------------------------------------------------------------------------------------

macro_rules! text_enum_sql {
    ($($name:ident),+ $(,)?) => {$(
        impl Type<Postgres> for $name {
            fn type_info() -> PgTypeInfo { <&str as Type<Postgres>>::type_info() }
            fn compatible(ty: &PgTypeInfo) -> bool { <&str as Type<Postgres>>::compatible(ty) }
        }
        impl PgHasArrayType for $name {
            fn array_type_info() -> PgTypeInfo { <&str as PgHasArrayType>::array_type_info() }
        }
        impl<'q> Encode<'q, Postgres> for $name {
            fn encode_by_ref(&self, buf: &mut PgArgumentBuffer) -> Result<sqlx::encode::IsNull, sqlx::error::BoxDynError> {
                <&str as Encode<'q, Postgres>>::encode_by_ref(&self.as_str(), buf)
            }
        }
        impl<'r> Decode<'r, Postgres> for $name {
            fn decode(value: PgValueRef<'r>) -> Result<Self, sqlx::error::BoxDynError> {
                let s = <&str as Decode<'r, Postgres>>::decode(value)?;
                Ok(s.parse::<$name>()?)
            }
        }
    )+};
}

text_enum_sql!(
    GameType,
    Conference,
    PlayoffResult,
    JobKind,
    JobStatus,
    ImportStatus,
    EloSource,
    ResultSource
);

// ---------------------------------------------------------------------------------------
// Row mapping
// ---------------------------------------------------------------------------------------

impl FromRow<'_, PgRow> for Franchise {
    fn from_row(r: &PgRow) -> Result<Self, sqlx::Error> {
        Ok(Self {
            id: r.try_get("id")?,
            key: r.try_get("key")?,
            name: r.try_get("name")?,
            active: r.try_get("active")?,
        })
    }
}

impl FromRow<'_, PgRow> for Team {
    fn from_row(r: &PgRow) -> Result<Self, sqlx::Error> {
        Ok(Self {
            id: r.try_get("id")?,
            code: r.try_get("code")?,
            franchise_id: r.try_get("franchise_id")?,
            name: r.try_get("name")?,
            first_season: r.try_get("first_season")?,
            last_season: r.try_get("last_season")?,
        })
    }
}

impl FromRow<'_, PgRow> for Season {
    fn from_row(r: &PgRow) -> Result<Self, sqlx::Error> {
        Ok(Self {
            season: r.try_get("season")?,
            label: r.try_get("label")?,
            team_count: r.try_get("team_count")?,
            game_count: r.try_get("game_count")?,
            playoff_game_count: r.try_get("playoff_game_count")?,
            play_in_game_count: r.try_get("play_in_game_count")?,
            cup_final_game_count: r.try_get("cup_final_game_count")?,
            computed_game_count: r.try_get("computed_game_count")?,
            archive_game_count: r.try_get("archive_game_count")?,
            scheduled_game_count: r.try_get("scheduled_game_count")?,
            regular_season_end: r.try_get::<Option<NaiveDate>, _>("regular_season_end")?,
            playoffs_start: r.try_get("playoffs_start")?,
            playoffs_end: r.try_get("playoffs_end")?,
            complete: r.try_get("complete")?,
        })
    }
}

impl FromRow<'_, PgRow> for Game {
    fn from_row(r: &PgRow) -> Result<Self, sqlx::Error> {
        Ok(Self {
            id: r.try_get("id")?,
            season: r.try_get("season")?,
            game_date: r.try_get("game_date")?,
            seq: r.try_get("seq")?,
            home_team_id: r.try_get("home_team_id")?,
            away_team_id: r.try_get("away_team_id")?,
            neutral: r.try_get("neutral")?,
            game_type: r.try_get("game_type")?,
            home_score: r.try_get("home_score")?,
            away_score: r.try_get("away_score")?,
            home_elo_pre: r.try_get("home_elo_pre")?,
            away_elo_pre: r.try_get("away_elo_pre")?,
            home_elo_post: r.try_get("home_elo_post")?,
            away_elo_post: r.try_get("away_elo_post")?,
            home_win_prob: r.try_get("home_win_prob")?,
            elo_source: r.try_get("elo_source")?,
            result_source: r.try_get("result_source")?,
            ref_home_elo_pre: r.try_get("ref_home_elo_pre")?,
            ref_away_elo_pre: r.try_get("ref_away_elo_pre")?,
            ref_home_elo_post: r.try_get("ref_home_elo_post")?,
            ref_away_elo_post: r.try_get("ref_away_elo_post")?,
        })
    }
}

impl FromRow<'_, PgRow> for TeamSeason {
    fn from_row(r: &PgRow) -> Result<Self, sqlx::Error> {
        Ok(Self {
            season: r.try_get("season")?,
            team_id: r.try_get("team_id")?,
            franchise_id: r.try_get("franchise_id")?,
            conference: r.try_get("conference")?,
            division: r.try_get("division")?,
            wins: r.try_get("wins")?,
            losses: r.try_get("losses")?,
            win_pct: r.try_get("win_pct")?,
            elo_start: r.try_get("elo_start")?,
            elo_regular_end: r.try_get("elo_regular_end")?,
            elo_pre_playoffs: r.try_get("elo_pre_playoffs")?,
            elo_end: r.try_get("elo_end")?,
            elo_peak: r.try_get("elo_peak")?,
            made_play_in: r.try_get("made_play_in")?,
            made_playoffs: r.try_get("made_playoffs")?,
            seed: r.try_get("seed")?,
            playoff_result: r.try_get("playoff_result")?,
        })
    }
}

impl FromRow<'_, PgRow> for TeamSeasonView {
    fn from_row(r: &PgRow) -> Result<Self, sqlx::Error> {
        Ok(Self {
            season: r.try_get("season")?,
            team_id: r.try_get("team_id")?,
            code: r.try_get("code")?,
            team_name: r.try_get("team_name")?,
            franchise_id: r.try_get("franchise_id")?,
            franchise_key: r.try_get("franchise_key")?,
            franchise_name: r.try_get("franchise_name")?,
            conference: r.try_get("conference")?,
            division: r.try_get("division")?,
            wins: r.try_get("wins")?,
            losses: r.try_get("losses")?,
            win_pct: r.try_get("win_pct")?,
            elo_start: r.try_get("elo_start")?,
            elo_regular_end: r.try_get("elo_regular_end")?,
            elo_pre_playoffs: r.try_get("elo_pre_playoffs")?,
            elo_end: r.try_get("elo_end")?,
            elo_peak: r.try_get("elo_peak")?,
            made_play_in: r.try_get("made_play_in")?,
            made_playoffs: r.try_get("made_playoffs")?,
            seed: r.try_get("seed")?,
            playoff_result: r.try_get("playoff_result")?,
        })
    }
}

impl FromRow<'_, PgRow> for ChampionshipView {
    fn from_row(r: &PgRow) -> Result<Self, sqlx::Error> {
        Ok(Self {
            season: r.try_get("season")?,
            champion_code: r.try_get("champion_code")?,
            champion_name: r.try_get("champion_name")?,
            franchise_key: r.try_get("franchise_key")?,
            franchise_name: r.try_get("franchise_name")?,
            runner_up_code: r.try_get("runner_up_code")?,
            source: r.try_get("source")?,
        })
    }
}

impl FromRow<'_, PgRow> for DataImport {
    fn from_row(r: &PgRow) -> Result<Self, sqlx::Error> {
        Ok(Self {
            id: r.try_get("id")?,
            source: r.try_get("source")?,
            source_url: r.try_get("source_url")?,
            file_name: r.try_get("file_name")?,
            file_sha256: r.try_get("file_sha256")?,
            format: r.try_get("format")?,
            status: r.try_get("status")?,
            rows_read: r.try_get("rows_read")?,
            games_inserted: r.try_get("games_inserted")?,
            games_updated: r.try_get("games_updated")?,
            games_unchanged: r.try_get("games_unchanged")?,
            rows_rejected: r.try_get("rows_rejected")?,
            min_season: r.try_get("min_season")?,
            max_season: r.try_get("max_season")?,
            error: r.try_get("error")?,
            notes: r.try_get("notes")?,
            started_at: r.try_get::<DateTime<Utc>, _>("started_at")?,
            finished_at: r.try_get("finished_at")?,
        })
    }
}

impl FromRow<'_, PgRow> for SimulationJob {
    fn from_row(r: &PgRow) -> Result<Self, sqlx::Error> {
        Ok(Self {
            id: r.try_get::<Uuid, _>("id")?,
            kind: r.try_get("kind")?,
            name: r.try_get("name")?,
            status: r.try_get("status")?,
            params: r.try_get("params")?,
            result: r.try_get("result")?,
            progress: r.try_get("progress")?,
            error: r.try_get("error")?,
            seed: r.try_get("seed")?,
            app_version: r.try_get("app_version")?,
            model_version: r.try_get("model_version")?,
            attempts: r.try_get("attempts")?,
            created_at: r.try_get("created_at")?,
            started_at: r.try_get("started_at")?,
            finished_at: r.try_get("finished_at")?,
            updated_at: r.try_get("updated_at")?,
        })
    }
}

impl FromRow<'_, PgRow> for TeamSeasonStat {
    fn from_row(r: &PgRow) -> Result<Self, sqlx::Error> {
        Ok(Self {
            season: r.try_get("season")?,
            code: r.try_get("code")?,
            name: r.try_get("name")?,
            franchise_key: r.try_get("franchise_key")?,
            games: r.try_get("games")?,
            peak: r.try_get("peak")?,
            peak_date: r.try_get("peak_date")?,
            low: r.try_get("low")?,
            low_date: r.try_get("low_date")?,
            wins: r.try_get::<Option<i32>, _>("wins")?.unwrap_or(0),
            losses: r.try_get::<Option<i32>, _>("losses")?.unwrap_or(0),
            elo_start: r.try_get("elo_start")?,
            elo_pre_playoffs: r.try_get("elo_pre_playoffs")?,
            elo_end: r.try_get("elo_end")?,
            playoff_result: r
                .try_get::<Option<PlayoffResult>, _>("playoff_result")?
                .unwrap_or(PlayoffResult::Missed),
            conference: r.try_get("conference")?,
            division: r.try_get("division")?,
        })
    }
}
