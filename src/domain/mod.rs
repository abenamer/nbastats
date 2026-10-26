//! Domain: types, invariants, mathematics and simulation engines. Nothing in this module
//! depends on Axum, SQLx, HTTP or Docker (REAS-ARCH-002).
pub mod backtesting;
pub mod cba;
pub mod continuation;
pub mod cup;
pub mod elo;
pub mod eras;
pub mod ids;
pub mod models;
pub mod parity;
pub mod players;
pub mod playoffs;
pub mod records;
pub mod reference;
pub mod simulation;
pub mod strength;
pub mod validation;
pub mod worth;
