//! What is currently cached for one issuer, and what it is evidence of.
//!
//! # Two clocks, deliberately not one
//!
//! An earlier version used a single interval for both jobs below. That
//! conflated a rate limit with a security claim, and it showed: during a
//! provider outage there was no bound on retries at all, because the only
//! timer was reset by *success*.
//!
//! - [`REFRESH_MIN_INTERVAL_SECONDS`] is amplification protection: how often
//!   we are willing to call the issuer, whatever the outcome.
//! - [`UNKNOWN_KID_FRESHNESS_SECONDS`] is a security semantic: how long a
//!   **successful** snapshot is recent enough to prove that an unfamiliar key
//!   does not exist.
//!
//! A failed attempt updates the first and never the second. A failure cannot
//! create negative evidence about a key.

use crate::{IssuerRegistration, KeySet, UnavailableReason};

/// How soon after *any* attempt another may be made for the same issuer.
///
/// Bounds calls to the issuer whether they are succeeding or failing. Without
/// it, a few thousand invented `kid` values during an outage become a few
/// thousand failing fetches aimed at a provider that is already unwell.
pub const REFRESH_MIN_INTERVAL_SECONDS: u64 = 10;

/// How long a successful snapshot proves an unfamiliar key does not exist.
///
/// Shorter than the staleness bound, which governs whether cached keys may
/// still *verify*. This governs something stronger: whether absence from the
/// snapshot is evidence enough to refuse a credential. Claiming a key does not
/// exist is a claim about the issuer's current state, so it expires quickly.
pub const UNKNOWN_KID_FRESHNESS_SECONDS: u64 = 30;

/// One issuer's cached trust material and the attempts made to refresh it.
#[derive(Default)]
pub(super) struct Entry {
    /// The last successful snapshot, if there has been one.
    pub(super) snapshot: Option<Snapshot>,

    /// When the issuer was last called, successfully or not.
    ///
    /// Separate from the snapshot's own timestamp precisely so that a failure
    /// suppresses the next call without ever looking like evidence.
    pub(super) last_attempt_at: Option<u64>,
}

impl Entry {
    /// Whether another call to the issuer is permitted yet.
    pub(super) fn may_refresh(&self, now: u64) -> bool {
        self.last_attempt_at
            .is_none_or(|at| now.saturating_sub(at) >= REFRESH_MIN_INTERVAL_SECONDS)
    }

    /// Which kind of unavailability this is, for the operator reading it.
    ///
    /// Both are `503`. The distinction is diagnostic: keys too old to trust is
    /// a different incident from keys that could not be fetched, and an
    /// operator seeing the first should be looking at how long the provider
    /// has been away rather than at the network.
    pub(super) fn unavailability(&self, registration: &IssuerRegistration, now: u64) -> UnavailableReason {
        if self
            .snapshot
            .as_ref()
            .is_some_and(|snapshot| snapshot.is_stale(registration, now))
        {
            UnavailableReason::KeysTooOld
        } else {
            UnavailableReason::KeysUnreachable
        }
    }
}

/// A key set that was successfully fetched, and when.
pub(super) struct Snapshot {
    /// The keys as fetched.
    pub(super) keys: KeySet,

    /// When the fetch succeeded, in unix seconds.
    pub(super) fetched_at: u64,
}

impl Snapshot {
    /// Whether this snapshot has aged past what its registration permits.
    ///
    /// Past the bound the keys are not merely old, they are untrusted: a key
    /// *removed* during a long outage would otherwise stay usable
    /// indefinitely, which is the one case where "keep serving" is wrong.
    pub(super) fn is_stale(&self, registration: &IssuerRegistration, now: u64) -> bool {
        now.saturating_sub(self.fetched_at) > registration.max_key_age_seconds
    }

    /// Whether this snapshot is recent enough to prove a key's **absence**.
    ///
    /// A stricter question than whether its keys may still verify a signature.
    /// Refusing a credential because a key is missing is a claim about what
    /// the issuer publishes *now*, and stale knowledge cannot support it.
    pub(super) fn proves_absence(&self, now: u64) -> bool {
        now.saturating_sub(self.fetched_at) < UNKNOWN_KID_FRESHNESS_SECONDS
    }
}
