//! Typestate example.
//!
//! The state of a `Job` is a type parameter. An operation that is invalid in
//! a state does not compile. The state parameter is bounded by the sealed
//! trait `JobState`, so the set of states is closed (REAS-STATE-002).
//!
//! A running job can complete:
//!
//! ```
//! use reas_typestate::Job;
//! let completed = Job::new(1).start().complete();
//! assert_eq!(completed.id(), 1);
//! ```
//!
//! A pending job cannot complete. `complete` is not defined for
//! `Job<Pending>`, so this is error E0599 (no method found):
//!
//! ```compile_fail,E0599
//! use reas_typestate::Job;
//! let _invalid = Job::new(1).complete();
//! ```
//!
//! A type outside the sealed set is not a job state. This is error E0277
//! (trait bound not satisfied):
//!
//! ```compile_fail,E0277
//! use reas_typestate::Job;
//! fn takes_job(_job: Job<String>) {}
//! ```
//!
//! A downstream crate cannot add a state by implementing `JobState`:
//!
//! ```compile_fail,E0277
//! use reas_typestate::JobState;
//! struct ExternalState;
//! impl JobState for ExternalState {}
//! ```
//!
//! Note on evidence: stable rustdoc accepts the error code after
//! `compile_fail` but does not check it. Only nightly rustdoc fails the test
//! when an expected error code is absent. Extra diagnostics can also occur.
//! On stable, these tests prove that the snippet does
//! not compile, not why.

use std::marker::PhantomData;

mod sealed {
    pub trait Sealed {}
}

/// Marker trait for the states of a [`Job`]. Only this crate can implement it.
pub trait JobState: sealed::Sealed {}

pub struct Pending;
pub struct Running;
pub struct Completed;

impl sealed::Sealed for Pending {}
impl sealed::Sealed for Running {}
impl sealed::Sealed for Completed {}

impl JobState for Pending {}
impl JobState for Running {}
impl JobState for Completed {}

pub struct Job<State: JobState> {
    id: u64,
    state: PhantomData<State>,
}

impl Job<Pending> {
    pub fn new(id: u64) -> Self {
        Self {
            id,
            state: PhantomData,
        }
    }

    pub fn start(self) -> Job<Running> {
        Job {
            id: self.id,
            state: PhantomData,
        }
    }
}

impl Job<Running> {
    pub fn complete(self) -> Job<Completed> {
        Job {
            id: self.id,
            state: PhantomData,
        }
    }
}

impl<State: JobState> Job<State> {
    pub fn id(&self) -> u64 {
        self.id
    }
}

#[cfg(test)]
mod tests {
    use super::{Completed, Job, Running};

    #[test]
    fn start_preserves_the_id() {
        let running: Job<Running> = Job::new(123).start();
        assert_eq!(running.id(), 123);
    }

    #[test]
    fn complete_preserves_the_id() {
        let completed: Job<Completed> = Job::new(123).start().complete();
        assert_eq!(completed.id(), 123);
    }

    #[test]
    fn a_running_job_can_complete() {
        assert_eq!(Job::new(123).start().complete().id(), 123);
    }
}
