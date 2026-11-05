//! Key rotation of the question box. With a management key configured, the application mints
//! its own completion key at the service when it starts, replaces it on a schedule and when
//! the service rejects it, retires the keys it minted before once the questions in flight are
//! done, and deletes its key when it stops. The policy is `domain::keys`; the service is the
//! `KeyProvisioner` port. No secret is kept here: the minted key goes to the model and nowhere
//! else, and the status carries times and counts only.

use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::Serialize;
use tracing::{info, warn};

use crate::application::ports::{KeyError, KeyProvisioner};
use crate::application::state::App;
use crate::domain::keys::{self, Policy, Reason};

/// How often the loop looks at the schedule and the keys to retire.
pub const TICK: Duration = Duration::from_secs(60);
/// How long the shutdown waits for the service to delete the key in use.
const STOP_TIMEOUT: Duration = Duration::from_secs(10);

/// What the rotation remembers between looks.
#[derive(Debug, Default)]
pub struct Rotation {
    state: Mutex<State>,
    /// Woken by a rejected request, so the loop rotates before its next tick.
    wake: tokio::sync::Notify,
}

#[derive(Debug, Default, Clone)]
struct State {
    /// When the key in use was minted; `None` while the configured key, or none, is in use.
    rotated_at: Option<DateTime<Utc>>,
    /// When a key was last minted or the minting last failed.
    last_attempt: Option<DateTime<Utc>>,
    /// The hash of the minted key in use.
    current_hash: Option<String>,
    rotations: u32,
    last_error: Option<String>,
    /// Keys to delete, each with the time its grace ends and the deletions tried so far.
    retiring: Vec<Retiring>,
    /// Keys given up on after the deletions allowed; the listing does not mark them again.
    abandoned: Vec<String>,
    /// A rotation asked for by a rejected request, taken by the loop.
    wanted: Option<Reason>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Retiring {
    hash: String,
    after: DateTime<Utc>,
    attempts: u32,
}

/// The rotation as the API reports it: no secret, no hash.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Status {
    pub enabled: bool,
    pub interval_hours: u64,
    pub rotations: u32,
    pub rotated_at: Option<DateTime<Utc>>,
    pub next_rotation: Option<DateTime<Utc>>,
    pub retiring: usize,
    pub last_error: Option<String>,
}

impl Rotation {
    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn status(&self, enabled: bool, policy: Policy) -> Status {
        let s = self.lock();
        Status {
            enabled,
            interval_hours: policy.interval_hours(),
            rotations: s.rotations,
            rotated_at: s.rotated_at,
            next_rotation: if enabled {
                policy.next(s.rotated_at)
            } else {
                None
            },
            retiring: s.retiring.len(),
            last_error: s.last_error.clone(),
        }
    }

    /// The service rejected the key in use: ask the loop for a fresh one, unless an attempt was
    /// made within the cooldown. Returns whether a rotation was asked for.
    pub fn rejected(&self, now: DateTime<Utc>) -> bool {
        let mut s = self.lock();
        if !keys::attempt_allowed(now, s.last_attempt) || s.wanted.is_some() {
            return false;
        }
        s.wanted = Some(Reason::Rejected);
        drop(s);
        self.wake.notify_one();
        true
    }

    /// Why the loop rotates at `now`, if it does: a rotation asked for, else the schedule, each
    /// outside the cooldown of the last attempt.
    fn reason_at(&self, policy: Policy, now: DateTime<Utc>) -> Option<Reason> {
        let mut s = self.lock();
        if !keys::attempt_allowed(now, s.last_attempt) {
            return None;
        }
        if let Some(r) = s.wanted.take() {
            return Some(r);
        }
        if policy.due(now, s.rotated_at) {
            return Some(if s.rotations == 0 {
                Reason::Start
            } else {
                Reason::Due
            });
        }
        None
    }
}

/// What the status says of a failed minting: the kind of failure, never the service's own
/// words, which the log keeps.
fn describe(e: &KeyError) -> String {
    match e {
        KeyError::Transport(_) => "the key service could not be reached; see the log".to_string(),
        KeyError::Rejected { status, .. } => {
            format!("the key service answered with status {status}; see the log")
        }
        KeyError::InvalidResponse(_) => {
            "the key service's answer could not be read; see the log".to_string()
        }
    }
}

/// How the keys are minted: the name prefix and the spending limit of each.
#[derive(Debug, Clone, Copy)]
pub struct Minting<'a> {
    pub prefix: &'a str,
    pub limit: Option<f64>,
}

/// Mint a fresh key named with `prefix` for `now`, hand its secret to `install`, and mark the
/// application's other keys at the service for retirement after the grace: the one replaced,
/// and any with the prefix left by an earlier run. A failed minting leaves the key in use and
/// records the failure.
pub async fn rotate<P: KeyProvisioner>(
    service: &P,
    install: impl FnOnce(String),
    rotation: &Rotation,
    minting: Minting<'_>,
    now: DateTime<Utc>,
    clock: impl Fn() -> DateTime<Utc>,
    stopping: impl Fn() -> bool,
) -> Result<(), KeyError> {
    let Minting { prefix, limit } = minting;
    rotation.lock().last_attempt = Some(now);
    let issued = match service.create(&keys::key_name(prefix, now), limit).await {
        Ok(k) => k,
        Err(e) => {
            rotation.lock().last_error = Some(describe(&e));
            return Err(e);
        }
    };
    install(issued.secret);
    // The grace runs from the install, since the minting takes time.
    let now = clock();
    {
        let mut s = rotation.lock();
        let previous = s.current_hash.replace(issued.hash.clone());
        s.rotated_at = Some(now);
        s.rotations += 1;
        s.last_error = None;
        if let Some(hash) = previous {
            mark(&mut s, hash, now);
        }
    }
    // At shutdown the listing is skipped: the stop deletes the key just minted, and the
    // next start retires the rest.
    if stopping() {
        return Ok(());
    }
    match service.list().await {
        Ok(list) => {
            let mut s = rotation.lock();
            for k in list {
                if keys::is_ours(prefix, &k.name) && k.hash != issued.hash {
                    mark(&mut s, k.hash, now);
                }
            }
        }
        Err(e) => {
            warn!(error = %e, "the account's keys could not be listed; older minted keys stay until the next rotation")
        }
    }
    Ok(())
}

fn mark(s: &mut State, hash: String, now: DateTime<Utc>) {
    if s.current_hash.as_deref() == Some(hash.as_str())
        || s.retiring.iter().any(|r| r.hash == hash)
        || s.abandoned.contains(&hash)
    {
        return;
    }
    s.retiring.push(Retiring {
        hash,
        after: keys::retire_at(now),
        attempts: 0,
    });
}

/// Delete the keys whose grace is over. A key the service no longer has counts as deleted; a
/// deletion that fails is tried again after another grace, a few times.
pub async fn retire<P: KeyProvisioner>(
    service: &P,
    rotation: &Rotation,
    now: DateTime<Utc>,
) -> usize {
    let due: Vec<Retiring> = {
        let mut s = rotation.lock();
        let (due, later): (Vec<_>, Vec<_>) = s.retiring.drain(..).partition(|r| r.after <= now);
        s.retiring = later;
        due
    };
    let mut deleted = 0;
    for mut r in due {
        match service.delete(&r.hash).await {
            Ok(()) => deleted += 1,
            Err(KeyError::Rejected { status: 404, .. }) => deleted += 1,
            Err(e) => {
                r.attempts += 1;
                if r.attempts >= keys::MAX_DELETE_ATTEMPTS {
                    warn!(error = %e, attempts = r.attempts, "a minted key could not be deleted; delete it at the service");
                    rotation.lock().abandoned.push(r.hash);
                } else {
                    warn!(error = %e, attempts = r.attempts, "a minted key could not be deleted; it is tried again");
                    r.after = keys::retire_at(now);
                    rotation.lock().retiring.push(r);
                }
            }
        }
    }
    deleted
}

/// The background loop: a key at start, then on the schedule and on request.
pub async fn run(app: Arc<App>) {
    let (Some(service), Some(model)) = (&app.keys, &app.model) else {
        return;
    };
    let policy = Policy::hours(app.config.openrouter_key_rotation_hours);
    let minting = Minting {
        prefix: app.config.openrouter_key_name.as_str(),
        limit: app.config.openrouter_key_limit,
    };
    loop {
        let now = Utc::now();
        if let Some(reason) = app.key_rotation.reason_at(policy, now) {
            match rotate(
                service,
                |secret| model.install_key(secret),
                &app.key_rotation,
                minting,
                now,
                Utc::now,
                || app.shutdown.is_cancelled(),
            )
            .await
            {
                Ok(()) => info!(
                    reason = reason.as_str(),
                    "question box key minted and installed"
                ),
                Err(e) => warn!(
                    reason = reason.as_str(),
                    error = %e,
                    in_use = model.has_key(),
                    "question box key not minted; tried again after the cooldown"
                ),
            }
        }
        if app.shutdown.is_cancelled() {
            break;
        }
        let deleted = retire(service, &app.key_rotation, Utc::now()).await;
        if deleted > 0 {
            info!(deleted, "older question box keys deleted");
        }
        tokio::select! {
            _ = tokio::time::sleep(TICK) => {}
            _ = app.key_rotation.wake.notified() => {}
            _ = app.shutdown.cancelled() => break,
        }
    }
}

/// At shutdown: delete the minted key in use and the ones still retiring, so a stopped
/// application leaves no live key of its own. A key that cannot be deleted now is retired at
/// the next start.
pub async fn stop(app: &App) {
    let Some(service) = &app.keys else {
        return;
    };
    let hashes: Vec<String> = {
        let mut s = app.key_rotation.lock();
        s.current_hash
            .take()
            .into_iter()
            .chain(s.retiring.drain(..).map(|r| r.hash))
            .collect()
    };
    if hashes.is_empty() {
        return;
    }
    let deletions = async {
        let mut deleted = 0;
        for hash in &hashes {
            match service.delete(hash).await {
                Ok(()) | Err(KeyError::Rejected { status: 404, .. }) => deleted += 1,
                Err(e) => {
                    warn!(error = %e, "a question box key could not be deleted at shutdown; the next start retires it")
                }
            }
        }
        deleted
    };
    match tokio::time::timeout(STOP_TIMEOUT, deletions).await {
        Ok(deleted) => info!(
            deleted,
            of = hashes.len(),
            "question box keys deleted at shutdown"
        ),
        Err(_) => warn!("the service did not answer in time; the next start retires the keys"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::ports::{IssuedKey, KeyRecord};
    use chrono::TimeZone;
    use std::sync::atomic::{AtomicBool, Ordering};

    const P: &str = keys::DEFAULT_NAME_PREFIX;
    const NO_LIMIT: Minting<'static> = Minting {
        prefix: P,
        limit: None,
    };

    fn at(h: u32, m: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 11, 5, h, m, 0).unwrap()
    }

    /// A service holding keys by hash and name; minting fails while `broken` is set.
    #[derive(Default)]
    struct Fake {
        keys: Mutex<Vec<KeyRecord>>,
        minted: Mutex<Vec<(String, Option<f64>)>>,
        deleted: Mutex<Vec<String>>,
        broken: AtomicBool,
        refuse_delete: AtomicBool,
    }

    impl KeyProvisioner for Fake {
        async fn create(&self, name: &str, limit: Option<f64>) -> Result<IssuedKey, KeyError> {
            if self.broken.load(Ordering::SeqCst) {
                return Err(KeyError::Rejected {
                    status: 401,
                    message: "bad management key".into(),
                });
            }
            let mut minted = self.minted.lock().unwrap();
            minted.push((name.to_string(), limit));
            let hash = format!("h{}", minted.len());
            self.keys.lock().unwrap().push(KeyRecord {
                hash: hash.clone(),
                name: name.to_string(),
                disabled: false,
            });
            Ok(IssuedKey {
                secret: format!("sk-{hash}"),
                hash,
            })
        }

        async fn list(&self) -> Result<Vec<KeyRecord>, KeyError> {
            Ok(self.keys.lock().unwrap().clone())
        }

        async fn delete(&self, hash: &str) -> Result<(), KeyError> {
            if self.refuse_delete.load(Ordering::SeqCst) {
                return Err(KeyError::Rejected {
                    status: 500,
                    message: "down".into(),
                });
            }
            let mut keys = self.keys.lock().unwrap();
            let Some(i) = keys.iter().position(|k| k.hash == hash) else {
                return Err(KeyError::Rejected {
                    status: 404,
                    message: "no such key".into(),
                });
            };
            keys.remove(i);
            self.deleted.lock().unwrap().push(hash.to_string());
            Ok(())
        }
    }

    fn fake_with(keys: &[(&str, &str)]) -> Fake {
        let f = Fake::default();
        *f.keys.lock().unwrap() = keys
            .iter()
            .map(|(h, n)| KeyRecord {
                hash: h.to_string(),
                name: n.to_string(),
                disabled: false,
            })
            .collect();
        f
    }

    #[tokio::test]
    async fn a_rotation_mints_installs_and_retires_the_applications_other_keys_only() {
        let f = fake_with(&[
            ("old", "nba-elo-simulator 2026-11-04T07:00Z"),
            ("laptop", "Laptop"),
        ]);
        let r = Rotation::default();
        let installed = Mutex::new(None);
        rotate(
            &f,
            |s| *installed.lock().unwrap() = Some(s),
            &r,
            Minting {
                prefix: P,
                limit: Some(5.0),
            },
            at(7, 0),
            || at(7, 0),
            || false,
        )
        .await
        .unwrap();
        assert_eq!(installed.lock().unwrap().as_deref(), Some("sk-h1"));
        assert_eq!(
            f.minted.lock().unwrap()[0],
            ("nba-elo-simulator 2026-11-05T07:00Z".to_string(), Some(5.0))
        );
        let status = r.status(true, Policy::hours(24));
        assert_eq!(status.rotations, 1);
        assert_eq!(status.rotated_at, Some(at(7, 0)));
        assert_eq!(
            status.next_rotation,
            Some(Utc.with_ymd_and_hms(2026, 11, 6, 7, 0, 0).unwrap())
        );
        assert_eq!(status.retiring, 1);
        // Nothing is deleted before the grace; then the old minted key goes, the laptop's stays.
        assert_eq!(retire(&f, &r, at(7, 1)).await, 0);
        assert_eq!(retire(&f, &r, at(7, 2)).await, 1);
        assert_eq!(*f.deleted.lock().unwrap(), vec!["old".to_string()]);
        assert_eq!(r.status(true, Policy::hours(24)).retiring, 0);
        // The second rotation retires the first minted key.
        rotate(&f, |_| {}, &r, NO_LIMIT, at(8, 0), || at(8, 0), || false)
            .await
            .unwrap();
        assert_eq!(retire(&f, &r, at(8, 2)).await, 1);
        assert_eq!(
            *f.deleted.lock().unwrap(),
            vec!["old".to_string(), "h1".to_string()]
        );
        let names: Vec<String> = f
            .keys
            .lock()
            .unwrap()
            .iter()
            .map(|k| k.name.clone())
            .collect();
        assert_eq!(names, vec!["Laptop", "nba-elo-simulator 2026-11-05T08:00Z"]);
        // No secret or hash in the status.
        let shown = serde_json::to_string(&r.status(true, Policy::hours(24))).unwrap();
        assert!(!shown.contains("sk-") && !shown.contains("h2"));
    }

    #[tokio::test]
    async fn a_failed_minting_keeps_the_key_in_use_and_waits_for_the_cooldown() {
        let f = Fake::default();
        f.broken.store(true, Ordering::SeqCst);
        let r = Rotation::default();
        let policy = Policy::hours(24);
        assert_eq!(r.reason_at(policy, at(7, 0)), Some(Reason::Start));
        let installed = AtomicBool::new(false);
        assert!(rotate(
            &f,
            |_| installed.store(true, Ordering::SeqCst),
            &r,
            Minting {
                prefix: P,
                limit: None,
            },
            at(7, 0),
            || at(7, 0),
            || false
        )
        .await
        .is_err());
        assert!(!installed.load(Ordering::SeqCst));
        let status = r.status(true, policy);
        assert_eq!(status.rotations, 0);
        assert_eq!(
            status.last_error.as_deref(),
            Some("the key service answered with status 401; see the log")
        );
        // Not again within the cooldown, by the schedule or by a rejection.
        assert_eq!(r.reason_at(policy, at(7, 9)), None);
        assert!(!r.rejected(at(7, 9)));
        assert_eq!(r.reason_at(policy, at(7, 10)), Some(Reason::Start));
        f.broken.store(false, Ordering::SeqCst);
        rotate(&f, |_| {}, &r, NO_LIMIT, at(7, 10), || at(7, 10), || false)
            .await
            .unwrap();
        assert_eq!(r.status(true, policy).last_error, None);
        // Then the schedule, not before.
        assert_eq!(r.reason_at(policy, at(23, 0)), None);
        assert_eq!(
            r.reason_at(policy, Utc.with_ymd_and_hms(2026, 11, 6, 7, 10, 0).unwrap()),
            Some(Reason::Due)
        );
    }

    #[tokio::test]
    async fn a_rejection_asks_for_one_rotation_outside_the_cooldown() {
        let f = Fake::default();
        let r = Rotation::default();
        let policy = Policy::hours(24);
        rotate(&f, |_| {}, &r, NO_LIMIT, at(7, 0), || at(7, 0), || false)
            .await
            .unwrap();
        assert!(!r.rejected(at(7, 5)));
        assert!(r.rejected(at(7, 10)));
        // A second rejection before the loop took the first asks for nothing more.
        assert!(!r.rejected(at(7, 11)));
        assert_eq!(r.reason_at(policy, at(7, 11)), Some(Reason::Rejected));
        assert_eq!(r.reason_at(policy, at(7, 11)), None);
    }

    #[tokio::test]
    async fn a_deletion_that_fails_is_tried_again_a_few_times_and_a_missing_key_counts_as_gone() {
        let f = fake_with(&[("old", "nba-elo-simulator 2026-11-04T07:00Z")]);
        let r = Rotation::default();
        rotate(&f, |_| {}, &r, NO_LIMIT, at(7, 0), || at(7, 0), || false)
            .await
            .unwrap();
        f.refuse_delete.store(true, Ordering::SeqCst);
        let mut now = at(7, 2);
        for _ in 0..keys::MAX_DELETE_ATTEMPTS - 1 {
            assert_eq!(retire(&f, &r, now).await, 0);
            assert_eq!(r.status(true, Policy::hours(24)).retiring, 1);
            now += chrono::Duration::minutes(2);
        }
        assert_eq!(retire(&f, &r, now).await, 0);
        assert_eq!(r.status(true, Policy::hours(24)).retiring, 0, "given up");
        // The next rotation's listing does not take the abandoned key up again.
        rotate(&f, |_| {}, &r, NO_LIMIT, at(8, 0), || at(8, 0), || false)
            .await
            .unwrap();
        assert_eq!(
            r.status(true, Policy::hours(24)).retiring,
            1,
            "the key replaced only"
        );
        // A key already gone at the service is counted as deleted, beside one deleted now.
        f.refuse_delete.store(false, Ordering::SeqCst);
        rotate(&f, |_| {}, &r, NO_LIMIT, at(9, 0), || at(9, 0), || false)
            .await
            .unwrap();
        f.keys.lock().unwrap().retain(|k| k.hash != "h1");
        assert_eq!(retire(&f, &r, at(9, 2)).await, 2);
        assert_eq!(*f.deleted.lock().unwrap(), vec!["h2".to_string()]);
        assert_eq!(r.status(true, Policy::hours(24)).retiring, 0);
    }
}
