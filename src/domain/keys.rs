//! Key rotation policy for the question box's OpenRouter key. The application mints its own
//! completion key from a provisioning key, replaces it on a schedule and when the service
//! rejects it, and retires the keys it minted before. This module holds the pure rules: how a
//! minted key is named and told from the account's other keys, when a fresh key is due, when a
//! rejection may force one, and how long a replaced key is kept for the requests still using it.

use chrono::{DateTime, Utc};
use std::time::Duration;

/// Every key the application mints is named with a prefix, so a later run can tell its own
/// keys from the account's others and retire them. This is the prefix when the environment
/// does not set one; two instances sharing one management key need different prefixes, or
/// each retires the other's key.
pub const DEFAULT_NAME_PREFIX: &str = "nba-elo-simulator";
/// The longest prefix the environment may set.
pub const MAX_PREFIX_CHARS: usize = 64;
/// Hours between two scheduled rotations when the environment does not say.
pub const DEFAULT_INTERVAL_HOURS: u32 = 24;
/// The shortest and longest schedule the environment may set, in hours (one hour to a year).
pub const MIN_INTERVAL_HOURS: u32 = 1;
pub const MAX_INTERVAL_HOURS: u32 = 24 * 365;
/// Two attempts to mint a key come at least this far apart, whether the second is forced by a
/// rejected request (status 401, the service's answer to a disabled or invalid key) or retries
/// a failed one: a key the service refuses for
/// another reason (a bad model, an account out of credits) cannot make the application mint a
/// key per question, and a management key the service refuses is tried a few times an hour.
pub const ATTEMPT_COOLDOWN: Duration = Duration::from_secs(10 * 60);
/// A key whose deletion keeps failing is given up after this many tries.
pub const MAX_DELETE_ATTEMPTS: u32 = 5;
/// A replaced key stays valid this long after its successor is installed, for the questions in
/// flight on it (a question runs at most 75 seconds).
pub const RETIRE_GRACE: Duration = Duration::from_secs(2 * 60);

/// A usable prefix: trimmed, not empty, at most `MAX_PREFIX_CHARS`, with no whitespace or
/// control character. A space is what separates the prefix from the rest of a key's name,
/// so a prefix without one can never be another prefix and a space. `None` when the text is
/// not one.
pub fn prefix(text: &str) -> Option<String> {
    let p = text.trim();
    if p.is_empty()
        || p.chars().count() > MAX_PREFIX_CHARS
        || p.chars().any(|c| c.is_whitespace() || c.is_control())
    {
        return None;
    }
    Some(p.to_string())
}

/// The name of a key minted at `now`: the prefix and the minute, in UTC.
pub fn key_name(prefix: &str, now: DateTime<Utc>) -> String {
    format!("{prefix} {}", now.format("%Y-%m-%dT%H:%MZ"))
}

/// Whether a key at the service was minted by the application, by its name: the prefix, a
/// space and the rest. A key named the bare prefix is not, so a key made by hand under the
/// project's name is left alone.
pub fn is_ours(prefix: &str, name: &str) -> bool {
    name.strip_prefix(prefix)
        .is_some_and(|rest| rest.starts_with(' '))
}

/// Why a rotation runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reason {
    /// The application started: the key in the environment, if any, may be the one the service
    /// disabled, and a minted key never outlives the process that minted it.
    Start,
    /// The schedule came due.
    Due,
    /// The service rejected the current key.
    Rejected,
}

impl Reason {
    pub fn as_str(self) -> &'static str {
        match self {
            Reason::Start => "start",
            Reason::Due => "due",
            Reason::Rejected => "rejected",
        }
    }
}

/// The schedule of the rotations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Policy {
    pub interval: Duration,
}

impl Policy {
    /// A policy of `hours` between rotations, held to the allowed range.
    pub fn hours(hours: u32) -> Self {
        let hours = hours.clamp(MIN_INTERVAL_HOURS, MAX_INTERVAL_HOURS);
        Self {
            interval: Duration::from_secs(u64::from(hours) * 3600),
        }
    }

    pub fn interval_hours(&self) -> u64 {
        self.interval.as_secs() / 3600
    }

    /// Whether a scheduled rotation is due at `now`, given when the current key was installed
    /// (`None` when none was minted yet: then one is due at once).
    pub fn due(&self, now: DateTime<Utc>, rotated_at: Option<DateTime<Utc>>) -> bool {
        match rotated_at {
            None => true,
            Some(t) => now.signed_duration_since(t).to_std().unwrap_or_default() >= self.interval,
        }
    }

    /// When the next scheduled rotation comes, for the status; `None` before the first one.
    pub fn next(&self, rotated_at: Option<DateTime<Utc>>) -> Option<DateTime<Utc>> {
        rotated_at
            .and_then(|t| t.checked_add_signed(chrono::Duration::from_std(self.interval).ok()?))
    }
}

/// Whether an attempt to mint a key may be made at `now`, given the last attempt: not within
/// the cooldown of it.
pub fn attempt_allowed(now: DateTime<Utc>, last_attempt: Option<DateTime<Utc>>) -> bool {
    match last_attempt {
        None => true,
        Some(t) => now.signed_duration_since(t).to_std().unwrap_or_default() >= ATTEMPT_COOLDOWN,
    }
}

/// When a key replaced at `now` may be deleted.
pub fn retire_at(now: DateTime<Utc>) -> DateTime<Utc> {
    now + chrono::Duration::from_std(RETIRE_GRACE).unwrap_or_else(|_| chrono::Duration::minutes(2))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn at(h: u32, m: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 11, 5, h, m, 0).unwrap()
    }

    #[test]
    fn minted_keys_are_named_with_the_prefix_and_told_apart() {
        let p = DEFAULT_NAME_PREFIX;
        let name = key_name(p, at(7, 48));
        assert_eq!(name, "nba-elo-simulator 2026-11-05T07:48Z");
        assert!(is_ours(p, &name));
        assert!(
            !is_ours(p, p),
            "a key named the bare prefix was made by hand"
        );
        assert!(!is_ours(p, "nba-elo-simulator-dev"));
        assert!(!is_ours(p, "nba-elo-simulator-dev 2026-11-05T07:48Z"));
        assert!(!is_ours(p, "Customer key"));
        assert!(!is_ours(p, ""));
        // Two instances with their own prefixes leave each other's keys alone.
        assert!(!is_ours("nba-elo-simulator-dev", &name));
        assert!(is_ours(
            "nba-elo-simulator-dev",
            "nba-elo-simulator-dev 2026-11-05T07:48Z"
        ));
    }

    #[test]
    fn a_prefix_is_trimmed_bounded_and_printable() {
        assert_eq!(prefix("  nba-dev ").as_deref(), Some("nba-dev"));
        assert_eq!(
            prefix("nba dev"),
            None,
            "a space would let two prefixes overlap"
        );
        assert_eq!(prefix(" "), None);
        assert_eq!(prefix("a\nb"), None);
        assert_eq!(prefix(&"x".repeat(64)).map(|p| p.len()), Some(64));
        assert_eq!(prefix(&"x".repeat(65)), None);
    }

    #[test]
    fn a_rotation_is_due_at_once_then_every_interval() {
        let p = Policy::hours(24);
        assert!(p.due(at(0, 0), None));
        assert!(!p.due(at(23, 59), Some(at(0, 0))));
        assert!(p.due(
            Utc.with_ymd_and_hms(2026, 11, 6, 0, 0, 0).unwrap(),
            Some(at(0, 0))
        ));
        assert_eq!(
            p.next(Some(at(0, 0))),
            Some(Utc.with_ymd_and_hms(2026, 11, 6, 0, 0, 0).unwrap())
        );
        assert_eq!(p.next(None), None);
        // A clock that went backwards is not due.
        assert!(!p.due(at(0, 0), Some(at(1, 0))));
    }

    #[test]
    fn the_schedule_is_held_to_the_allowed_range() {
        assert_eq!(Policy::hours(0).interval_hours(), 1);
        assert_eq!(Policy::hours(24).interval_hours(), 24);
        assert_eq!(Policy::hours(100_000).interval_hours(), 24 * 365);
    }

    #[test]
    fn an_attempt_is_allowed_outside_the_cooldown_only() {
        assert!(attempt_allowed(at(7, 0), None));
        assert!(!attempt_allowed(at(7, 9), Some(at(7, 0))));
        assert!(attempt_allowed(at(7, 10), Some(at(7, 0))));
        assert_eq!(retire_at(at(7, 0)), at(7, 2));
    }
}
