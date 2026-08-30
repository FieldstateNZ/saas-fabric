//! What is currently cached for one issuer, and when it stops being trusted.

use crate::{IssuerRegistration, KeySet};

/// How soon after a refresh another one may be attempted for the same issuer.
///
/// The cache's per-issuer lock serialises refreshes; it does not *bound* them.
/// Without this interval a few thousand requests carrying random `kid` values
/// each miss the cache and each provoke a fetch — one at a time, but all of
/// them — turning the verifier into an amplifier pointed at the identity
/// provider it depends on.
///
/// Inside the interval an unfamiliar `kid` is refused without a fetch, which
/// is correct as well as cheap: the key set was confirmed seconds ago and does
/// not publish it. Ten seconds keeps a genuine rotation quick to pick up.
pub(super) const MIN_REFRESH_INTERVAL_SECONDS: u64 = 10;

/// A key set and when it was read.
pub(super) struct Cached {
    /// The keys as last read.
    pub(super) keys: KeySet,

    /// When they were read, in unix seconds.
    pub(super) fetched_at: u64,
}

impl Cached {
    /// Whether this set has aged past what its registration permits.
    ///
    /// Past the bound the keys are not merely old, they are untrusted: a key
    /// *removed* during a long provider outage would otherwise stay usable
    /// indefinitely, which is the one case where "keep serving" is the wrong
    /// instinct.
    pub(super) fn is_stale(&self, registration: &IssuerRegistration, now: u64) -> bool {
        now.saturating_sub(self.fetched_at) > registration.max_key_age_seconds
    }

    /// Whether this set was confirmed too recently to be worth re-fetching.
    pub(super) fn refreshed_recently(&self, now: u64) -> bool {
        now.saturating_sub(self.fetched_at) < MIN_REFRESH_INTERVAL_SECONDS
    }
}
