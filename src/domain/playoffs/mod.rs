//! Engine B: Elo playoff simulator (bracket model, exact and Monte Carlo evaluation),
//! historical bracket reconstruction, standings, seeding rules and the full-season
//! simulator with play-in tournament.
pub mod bracket;
pub mod reconstruct;
pub mod season;
pub mod seeding;
pub mod standings;

pub const MODEL_VERSION: &str = "elo-playoff-1.0";
pub const SEASON_MODEL_VERSION: &str = "elo-season-1.4";
