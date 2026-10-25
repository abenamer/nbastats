//! Identifier newtypes (REAS-RUST-001). A `TeamId` cannot be confused with a `FranchiseId`,
//! and a `SeasonYear` is always inside the range the application supports.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

/// Database key of a team code (one row per code and era, e.g. SEA and OKC).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TeamId(i32);

impl TeamId {
    pub const fn new(raw: i32) -> Self {
        Self(raw)
    }
    pub const fn as_i32(self) -> i32 {
        self.0
    }
}

impl fmt::Display for TeamId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "team#{}", self.0)
    }
}

/// Database key of a player (one row per player of the player source).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PlayerId(i32);

impl PlayerId {
    pub const fn new(raw: i32) -> Self {
        Self(raw)
    }
    pub const fn as_i32(self) -> i32 {
        self.0
    }
}

impl fmt::Display for PlayerId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "player#{}", self.0)
    }
}

/// Database key of a franchise (the organisation through relocations and renamings).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct FranchiseId(i32);

impl FranchiseId {
    pub const fn new(raw: i32) -> Self {
        Self(raw)
    }
    pub const fn as_i32(self) -> i32 {
        self.0
    }
}

/// An NBA season named by the calendar year in which it ends: 2004 is 2003-04.
///
/// The first NBA (BAA) season ended in 1947. The upper bound keeps obviously wrong input out
/// of the engines; raise it when the data catches up.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "i32", into = "i32")]
pub struct SeasonYear(i32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("season {0} is outside the supported range {min}-{max}", min = SeasonYear::MIN.0, max = SeasonYear::MAX.0)]
pub struct InvalidSeason(pub i32);

impl SeasonYear {
    pub const MIN: SeasonYear = SeasonYear(1947);
    pub const MAX: SeasonYear = SeasonYear(2100);
    /// First season with conference alignment data, bracket reconstruction and forecasts.
    pub const FIRST_MODERN: SeasonYear = SeasonYear(2003);

    pub const fn try_new(end_year: i32) -> Result<Self, InvalidSeason> {
        if end_year >= Self::MIN.0 && end_year <= Self::MAX.0 {
            Ok(Self(end_year))
        } else {
            Err(InvalidSeason(end_year))
        }
    }

    /// For literals that are known to be valid (reference tables, tests). Panics otherwise.
    pub const fn of(end_year: i32) -> Self {
        match Self::try_new(end_year) {
            Ok(s) => s,
            Err(_) => panic!("season literal outside the supported range"),
        }
    }

    pub const fn end_year(self) -> i32 {
        self.0
    }

    pub const fn start_year(self) -> i32 {
        self.0 - 1
    }

    /// "2003-04"
    pub fn label(self) -> String {
        format!("{}-{:02}", self.0 - 1, self.0 % 100)
    }

    pub fn next(self) -> Option<SeasonYear> {
        Self::try_new(self.0 + 1).ok()
    }

    pub fn previous(self) -> Option<SeasonYear> {
        Self::try_new(self.0 - 1).ok()
    }

    /// Inclusive range of seasons.
    pub fn range(from: SeasonYear, to: SeasonYear) -> impl Iterator<Item = SeasonYear> {
        (from.0..=to.0).map(SeasonYear)
    }

    /// Number of seasons from `from` to `self`, inclusive (0 when `self` is earlier).
    pub fn seasons_since(self, from: SeasonYear) -> usize {
        (self.0 - from.0 + 1).max(0) as usize
    }
}

impl TryFrom<i32> for SeasonYear {
    type Error = InvalidSeason;
    fn try_from(value: i32) -> Result<Self, Self::Error> {
        Self::try_new(value)
    }
}

impl From<SeasonYear> for i32 {
    fn from(s: SeasonYear) -> i32 {
        s.0
    }
}

impl fmt::Display for SeasonYear {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for SeasonYear {
    type Err = InvalidSeason;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let raw: i32 = s.trim().parse().map_err(|_| InvalidSeason(0))?;
        Self::try_new(raw)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn season_bounds_and_labels() {
        assert!(SeasonYear::try_new(1946).is_err());
        assert!(SeasonYear::try_new(2101).is_err());
        assert_eq!(SeasonYear::of(2004).label(), "2003-04");
        assert_eq!(SeasonYear::of(2000).label(), "1999-00");
        assert_eq!(SeasonYear::of(2004).start_year(), 2003);
        assert_eq!(
            SeasonYear::range(SeasonYear::of(2004), SeasonYear::of(2006)).count(),
            3
        );
        assert_eq!(SeasonYear::of(2006).seasons_since(SeasonYear::of(2004)), 3);
        assert_eq!(SeasonYear::MAX.next(), None);
        assert_eq!("2016".parse::<SeasonYear>(), Ok(SeasonYear::of(2016)));
    }

    #[test]
    fn serde_validates() {
        assert!(serde_json::from_str::<SeasonYear>("1900").is_err());
        assert_eq!(
            serde_json::from_str::<SeasonYear>("2016").unwrap(),
            SeasonYear::of(2016)
        );
        assert_eq!(
            serde_json::to_string(&SeasonYear::of(2016)).unwrap(),
            "2016"
        );
        assert_eq!(serde_json::to_string(&TeamId::new(7)).unwrap(), "7");
    }
}
