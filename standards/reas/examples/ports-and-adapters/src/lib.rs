//! Ports and adapters with one visible code path.
//!
//! inbound adapter (`cli`) → use case (`application::FindTeam`) → port
//! (`application::TeamRepository`) → outbound adapter
//! (`adapters::InMemoryTeamRepository`).
//!
//! `composition::build_find_team` is the composition root. In a binary crate,
//! `main` would call it.
//!
//! The use case takes an adapter type parameter (REAS-ARCH-006). The
//! port returns a `Result` because a real repository can fail, even though
//! the in-memory adapter here cannot (REAS-RUST-005).

pub mod domain {
    use std::fmt;
    use std::num::NonZeroU32;

    /// Identifier of a team. Zero is reserved and is not a legal id.
    ///
    /// The standard library already has a type for this invariant, so the
    /// newtype wraps `NonZeroU32` instead of checking zero by hand. The field
    /// is private, so the tuple constructor is not part of the public API
    /// (REAS-RUST-001). All public construction paths preserve the invariant.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct TeamId(NonZeroU32);

    /// Error from `TeamId::new`.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum InvalidTeamId {
        Zero,
    }

    impl fmt::Display for InvalidTeamId {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            match self {
                InvalidTeamId::Zero => f.write_str("team id must not be zero"),
            }
        }
    }

    impl std::error::Error for InvalidTeamId {}

    impl TeamId {
        /// Smart constructor. It maps the std `None` to the domain error.
        pub fn new(raw: u32) -> Result<Self, InvalidTeamId> {
            NonZeroU32::new(raw).map(Self).ok_or(InvalidTeamId::Zero)
        }

        pub fn get(self) -> u32 {
            self.0.get()
        }
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct Team {
        pub id: TeamId,
        pub name: String,
    }
}

pub mod application {
    use std::fmt;

    use crate::domain::{Team, TeamId};

    /// Outbound port. The application defines it; an adapter implements it.
    pub trait TeamRepository {
        fn find(&self, id: TeamId) -> Result<Option<Team>, RepositoryError>;
    }

    /// Typed error for the repository port.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum RepositoryError {
        Unavailable(String),
    }

    impl fmt::Display for RepositoryError {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            match self {
                RepositoryError::Unavailable(reason) => {
                    write!(f, "repository unavailable: {reason}")
                }
            }
        }
    }

    impl std::error::Error for RepositoryError {}

    /// Use case. `TeamRepository` is the port; `R` is the adapter type.
    /// This direct delegation establishes the application boundary (REAS-ARCH-001).
    pub struct FindTeam<R> {
        repository: R,
    }

    impl<R: TeamRepository> FindTeam<R> {
        pub fn new(repository: R) -> Self {
            Self { repository }
        }

        pub fn execute(&self, id: TeamId) -> Result<Option<Team>, RepositoryError> {
            self.repository.find(id)
        }
    }
}

pub mod adapters {
    use std::collections::HashMap;

    use crate::application::{RepositoryError, TeamRepository};
    use crate::domain::{Team, TeamId};

    /// Outbound adapter backed by a `HashMap`. It cannot fail, so `find`
    /// always returns `Ok`.
    pub struct InMemoryTeamRepository {
        teams: HashMap<TeamId, Team>,
    }

    impl InMemoryTeamRepository {
        pub fn new(teams: impl IntoIterator<Item = Team>) -> Self {
            let teams = teams.into_iter().map(|team| (team.id, team)).collect();
            Self { teams }
        }
    }

    impl TeamRepository for InMemoryTeamRepository {
        fn find(&self, id: TeamId) -> Result<Option<Team>, RepositoryError> {
            Ok(self.teams.get(&id).cloned())
        }
    }
}

pub mod cli {
    use crate::application::{FindTeam, TeamRepository};
    use crate::domain::TeamId;

    /// Inbound adapter. It parses text, calls the use case, and formats the
    /// result. It contains no domain logic.
    ///
    /// This library example returns rendered text. A CLI binary must also set
    /// a failure exit status when the command fails (REAS-RUST-005).
    pub fn find_team_command<R: TeamRepository>(find_team: &FindTeam<R>, raw_id: &str) -> String {
        let Ok(raw) = raw_id.parse::<u32>() else {
            return format!("invalid team id: expected an unsigned 32-bit integer, got {raw_id:?}");
        };
        let id = match TeamId::new(raw) {
            Ok(id) => id,
            Err(err) => return format!("invalid team id: {err}"),
        };
        match find_team.execute(id) {
            Ok(Some(team)) => format!("team {}: {}", team.id.get(), team.name),
            Ok(None) => format!("team {} not found", id.get()),
            Err(err) => format!("error: {err}"),
        }
    }
}

pub mod composition {
    use crate::adapters::InMemoryTeamRepository;
    use crate::application::FindTeam;
    use crate::domain::Team;

    /// Composition root. This is the one place that selects the concrete
    /// adapter and connects it to the use case.
    pub fn build_find_team(
        teams: impl IntoIterator<Item = Team>,
    ) -> FindTeam<InMemoryTeamRepository> {
        let repository = InMemoryTeamRepository::new(teams);
        FindTeam::new(repository)
    }
}

#[cfg(test)]
mod tests {
    use crate::application::{FindTeam, RepositoryError, TeamRepository};
    use crate::cli::find_team_command;
    use crate::composition::build_find_team;
    use crate::domain::{InvalidTeamId, Team, TeamId};

    fn warriors() -> Team {
        Team {
            id: TeamId::new(3).expect("3 is not zero"),
            name: "Warriors".to_owned(),
        }
    }

    fn find_team() -> FindTeam<crate::adapters::InMemoryTeamRepository> {
        build_find_team([warriors()])
    }

    #[test]
    fn team_id_rejects_zero() {
        assert_eq!(TeamId::new(0), Err(InvalidTeamId::Zero));
    }

    #[test]
    fn find_returns_the_stored_team() {
        let id = TeamId::new(3).expect("3 is not zero");
        assert_eq!(find_team().execute(id), Ok(Some(warriors())));
    }

    #[test]
    fn find_returns_none_for_an_unknown_id() {
        let id = TeamId::new(9).expect("9 is not zero");
        assert_eq!(find_team().execute(id), Ok(None));
    }

    #[test]
    fn cli_formats_a_found_team() {
        assert_eq!(find_team_command(&find_team(), "3"), "team 3: Warriors");
    }

    #[test]
    fn cli_reports_a_missing_team() {
        assert_eq!(find_team_command(&find_team(), "9"), "team 9 not found");
    }

    #[test]
    fn cli_rejects_input_that_is_not_a_number() {
        assert_eq!(
            find_team_command(&find_team(), "abc"),
            "invalid team id: expected an unsigned 32-bit integer, got \"abc\""
        );
    }

    #[test]
    fn cli_rejects_a_zero_id() {
        assert_eq!(
            find_team_command(&find_team(), "0"),
            "invalid team id: team id must not be zero"
        );
    }

    struct UnavailableRepository;

    impl TeamRepository for UnavailableRepository {
        fn find(&self, _id: TeamId) -> Result<Option<Team>, RepositoryError> {
            Err(RepositoryError::Unavailable("offline".to_owned()))
        }
    }

    #[test]
    fn use_case_propagates_repository_failure() {
        let use_case = FindTeam::new(UnavailableRepository);
        let id = TeamId::new(3).expect("3 is not zero");
        assert_eq!(
            use_case.execute(id),
            Err(RepositoryError::Unavailable("offline".to_owned()))
        );
    }

    #[test]
    fn cli_formats_repository_failure() {
        let use_case = FindTeam::new(UnavailableRepository);
        assert_eq!(
            find_team_command(&use_case, "3"),
            "error: repository unavailable: offline"
        );
    }

    #[test]
    fn cli_rejects_a_negative_number() {
        assert_eq!(
            find_team_command(&find_team(), "-1"),
            "invalid team id: expected an unsigned 32-bit integer, got \"-1\""
        );
    }

    #[test]
    fn cli_rejects_a_number_above_u32_max() {
        assert_eq!(
            find_team_command(&find_team(), "4294967296"),
            "invalid team id: expected an unsigned 32-bit integer, got \"4294967296\""
        );
    }

    #[test]
    fn team_id_accepts_u32_max() {
        assert_eq!(
            TeamId::new(u32::MAX).expect("u32::MAX is not zero").get(),
            u32::MAX
        );
    }
}
