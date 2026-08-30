//! Which key a token may be verified against, and when trust must be refreshed.
//!
//! # The rule every branch here serves
//!
//! **An unknown `kid` is a refusal only when a sufficiently fresh, successfully
//! fetched snapshot positively establishes that the key is absent.** If the
//! verifier cannot obtain trust material fresh enough to make that claim, the
//! answer is that it does not know — never that the caller is unauthenticated.
//!
//! A failed refresh is an availability problem: it must never become
//! permission to try another key, skip a check, or serve as evidence about
//! one. The two windows below live in [`crate::windows`].
//!
//! ```text
//! unknown kid
//!   ├─ fresh successful snapshot, key absent      → refused
//!   └─ no snapshot fresh enough
//!        ├─ a call is permitted
//!        │    ├─ success, key present             → verify
//!        │    ├─ success, key absent              → refused
//!        │    └─ failure                          → unavailable
//!        └─ a call is suppressed by the cooldown  → unavailable
//! ```
//!
//! A token can move from refused to unavailable as evidence ages: the answer
//! follows the verifier's evidence, not the token.

#[cfg(test)]
mod cache_tests;
mod held;

use std::collections::HashMap;
use std::sync::Arc;

use fabric_core::Clock;
use jsonwebtoken::DecodingKey;
use tokio::sync::Mutex;

use crate::{IssuerRegistration, KeySource, RefusalReason, VerificationError};

use held::{Entry, Snapshot};

/// The keys this verifier is currently willing to trust, per issuer.
pub struct KeyCache {
    /// Where key sets are read from.
    source: Arc<dyn KeySource>,

    /// The clock every window is measured against.
    clock: Arc<dyn Clock>,

    /// One lock per issuer, which serialises refreshes for it.
    entries: Mutex<HashMap<String, Arc<Mutex<Entry>>>>,
}

impl KeyCache {
    /// Builds a cache over a key source.
    #[must_use]
    pub fn new(source: Arc<dyn KeySource>, clock: Arc<dyn Clock>) -> Self {
        Self {
            source,
            clock,
            entries: Mutex::new(HashMap::new()),
        }
    }

    /// Runs `use_key` against the key this issuer currently publishes as `kid`.
    ///
    /// The key is never handed out: doing so would mean holding it after the
    /// lock that decided it was current had been released.
    ///
    /// # Errors
    ///
    /// [`VerificationError::Refused`] only when a fresh successful snapshot
    /// says the issuer does not publish the id, and
    /// [`VerificationError::Unavailable`] whenever that could not be
    /// established.
    pub async fn with_key<R>(
        &self,
        registration: &IssuerRegistration,
        key_id: &str,
        use_key: impl FnOnce(&DecodingKey) -> R,
    ) -> Result<R, VerificationError> {
        let handle = self.entry_for(&registration.issuer).await;
        let mut entry = handle.lock().await;
        let now = self.clock.now_unix_seconds();

        let usable = entry
            .snapshot
            .as_ref()
            .filter(|held| !held.is_stale(registration, now));

        if let Some(snapshot) = usable {
            // Cached and usable: served without any call, which is what lets
            // an ordinary provider blip pass unnoticed.
            if let Some(key) = snapshot.keys.get(key_id) {
                return Ok(use_key(key));
            }

            // Absent from a snapshot recent enough to prove it. The only
            // branch that refuses a credential over a key, and what stops a
            // flood of invented ids becoming a flood of calls.
            if snapshot.proves_absence(now) {
                return Err(VerificationError::Refused(RefusalReason::UnknownKey));
            }
        }

        // Nothing usable, or nothing recent enough to speak for itself.
        if !entry.may_refresh(now) {
            // Suppressed by the cooldown. Whatever is cached could not answer
            // above, and a call we are not making cannot produce evidence.
            return Err(VerificationError::Unavailable(
                entry.unavailability(registration, now),
            ));
        }

        entry.last_attempt_at = Some(now);

        match self.source.fetch(&registration.jwks_uri).await {
            Ok(keys) => {
                // Decided against the set just fetched, then stored. Deciding
                // after storing would mean reading back through an `Option`
                // that cannot be empty, and writing a branch for an impossible
                // state is how a real one gets missed later.
                let outcome = keys.get(key_id).map_or_else(
                    || Err(VerificationError::Refused(RefusalReason::UnknownKey)),
                    |key| Ok(use_key(key)),
                );

                entry.snapshot = Some(Snapshot {
                    keys,
                    fetched_at: now,
                });

                outcome
            }

            // The call failed, so nothing was learned. The snapshot is left
            // exactly as it was: a failure must never age into evidence.
            Err(_) => Err(VerificationError::Unavailable(
                entry.unavailability(registration, now),
            )),
        }
    }

    /// The lock for one issuer, created on first use.
    async fn entry_for(&self, issuer: &str) -> Arc<Mutex<Entry>> {
        let mut entries = self.entries.lock().await;

        Arc::clone(
            entries
                .entry(issuer.to_owned())
                .or_insert_with(|| Arc::new(Mutex::new(Entry::default()))),
        )
    }
}
