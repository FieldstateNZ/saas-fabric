//! What the last sweep observed, and what it means.

use std::sync::RwLock;

use crate::integration::IntegrationStatus;
use crate::repository::RepositoryError;

/// What a read of desired state showed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Observation {
    /// Desired state was read.
    Read,

    /// Nothing is connected, so there was nothing to read.
    NotConfigured,

    /// The platform's credential was refused.
    Refused,

    /// Something else went wrong.
    Failed,
}

impl Observation {
    /// How a repository failure is classified for reporting.
    ///
    /// `NotPermitted` is the only one that becomes [`Refused`](Self::Refused),
    /// because it is the only one an operator resolves by reconnecting. A
    /// rejected request and an unreachable host both need somebody to look at
    /// the platform, not to re-run the install flow.
    #[must_use]
    pub fn of(error: &RepositoryError) -> Self {
        match error {
            RepositoryError::NotConfigured => Self::NotConfigured,
            RepositoryError::NotPermitted => Self::Refused,
            _ => Self::Failed,
        }
    }
}

/// The last thing a sweep saw when it read desired state.
///
/// Deliberately holds one observation rather than a history. The console asks
/// "can the platform read desired state right now"; a log answers "what has
/// happened over time", and it is already the place to look for that.
#[derive(Default)]
pub struct IntegrationHealth {
    /// The last observation and when it was made, if a sweep has run.
    last: RwLock<Option<(Observation, u64)>>,

    /// When desired state was last read successfully.
    ///
    /// Kept separately from `last` so that a currently-failing integration can
    /// still say when it last worked, which is usually the first question
    /// asked about one.
    last_success: RwLock<Option<u64>>,
}

impl IntegrationHealth {
    /// A health record with nothing observed yet.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Records what a sweep saw.
    pub fn record(&self, observation: Observation, at: u64) {
        Self::store(&self.last, Some((observation, at)));

        if observation == Observation::Read {
            Self::store(&self.last_success, Some(at));
        }
    }

    /// The status to report, given whether anything is bound.
    ///
    /// `configured` comes from the binding rather than from an observation,
    /// so a platform that has just connected a repository reports
    /// `not_configured` for no longer than it takes the first sweep to run.
    #[must_use]
    pub fn status(&self, configured: bool) -> IntegrationStatus {
        if !configured {
            return IntegrationStatus::NotConfigured;
        }

        match Self::load(&self.last) {
            // Bound, but no sweep has read it yet. Reported as connected
            // rather than as an error: nothing has gone wrong, and a status
            // that showed a fault for the first few seconds of every restart
            // would be a status nobody trusts.
            None | Some((Observation::Read, _)) => IntegrationStatus::Connected,
            Some((Observation::NotConfigured, _)) => IntegrationStatus::NotConfigured,
            Some((Observation::Refused, _)) => IntegrationStatus::Invalid,
            Some((Observation::Failed, _)) => IntegrationStatus::Error,
        }
    }

    /// When desired state was last read successfully.
    #[must_use]
    pub fn last_success(&self) -> Option<u64> {
        Self::load(&self.last_success)
    }

    /// Reads a slot, treating a poisoned lock as the value it holds.
    fn load<T: Copy>(slot: &RwLock<Option<T>>) -> Option<T> {
        match slot.read() {
            Ok(held) => *held,
            Err(poisoned) => *poisoned.into_inner(),
        }
    }

    /// Writes a slot, treating a poisoned lock as writable.
    fn store<T>(slot: &RwLock<Option<T>>, value: Option<T>) {
        match slot.write() {
            Ok(mut held) => *held = value,
            Err(poisoned) => *poisoned.into_inner() = value,
        }
    }
}
