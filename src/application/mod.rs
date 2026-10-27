//! Application: use cases and the ports they require. Use cases coordinate domain engines
//! with ports; adapters implement the ports (REAS-ARCH-001).
pub mod ask;
pub mod ask_players;
pub mod boxscore;
pub mod continuation;
pub mod dashboard;
pub mod dynasty;
pub mod home_advantage;
pub mod imports;
pub mod jobs;
pub mod players;
pub mod ports;
pub mod recompute;
pub mod recovery;
pub mod reports;
pub mod results_sync;
pub mod rookie;
pub mod scheduler;
pub mod state;
pub mod strength;
pub mod worth;
