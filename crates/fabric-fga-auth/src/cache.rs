//! Which key a token may be verified against, and when trust must be refreshed.
//!
//! # The rule a library default gets wrong
//!
//! A refresh that fails is an **availability** problem. It must never become
//! permission to try another key, skip a check, or serve on keys nobody has
//! confirmed. Every branch below produces a key that is genuinely current,
//! refuses the credential, or reports that trust could not be established —
//! and never anything else (ADR 0016).

#[cfg(test)]
mod cache_tests;
mod held;

use std::collections::HashMap;
use std::sync::Arc;

use fabric_core::Clock;
use jsonwebtoken::DecodingKey;
use tokio::sync::Mutex;

use crate::{IssuerRegistration, KeySource, RefusalReason, UnavailableReason, VerificationError};

use held::Cached;

/// The keys this verifier is currently willing to trust, per issuer.
pub struct KeyCache {
    /// Where key sets are read from.
    source: Arc<dyn KeySource>,

    /// The clock staleness is measured against.
    clock: Arc<dyn Clock>,

    /// One lock per issuer, which is what coalesces refreshes: a second
    /// request for the same issuer waits on the first rather than starting its
    /// own fetch. Without it, a few thousand random `kid` values turn this
    /// process into a fetch amplifier pointed at the identity provider.
    entries: Mutex<HashMap<String, Arc<Mutex<Option<Cached>>>>>,
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
    /// The key is never handed out, because handing it out would mean holding
    /// it after the lock that decided it was current had been released.
    ///
    /// # Errors
    ///
    /// [`VerificationError::Refused`] when the issuer genuinely does not
    /// publish the id, and [`VerificationError::Unavailable`] when that could
    /// not be established.
    pub async fn with_key<R>(
        &self,
        registration: &IssuerRegistration,
        key_id: &str,
        use_key: impl FnOnce(&DecodingKey) -> R,
    ) -> Result<R, VerificationError> {
        let entry = self.entry_for(&registration.issuer).await;
        let mut cached = entry.lock().await;

        let now = self.clock.now_unix_seconds();
        let mut stale = false;

        if let Some(held) = cached.as_ref() {
            stale = held.is_stale(registration, now);

            if !stale {
                // Cached and current: served without any fetch, which is what
                // lets an ordinary provider blip pass unnoticed.
                if let Some(key) = held.keys.get(key_id) {
                    return Ok(use_key(key));
                }

                // Unfamiliar id, but this key set was confirmed moments ago
                // and does not publish it. Refuse rather than re-fetch: this
                // is the branch that bounds amplification.
                if held.refreshed_recently(now) {
                    return Err(VerificationError::Refused(RefusalReason::UnknownKey));
                }
            }
        }

        // Nothing cached, cached-but-too-old, or an id worth one more look:
        // one refresh, holding this issuer's lock so concurrent callers wait
        // on it rather than piling on.
        match self.source.fetch(&registration.jwks_uri).await {
            Ok(keys) => {
                // Decided against the set just fetched, then cached. Deciding
                // after storing would mean reading back through an `Option`
                // that cannot be empty, and writing a branch for a state that
                // cannot happen is how a real one gets missed later.
                let outcome = keys.get(key_id).map_or_else(
                    // A key set known to be current does not publish this id,
                    // so the token was not signed by this issuer. The caller's
                    // problem, not the platform's.
                    || Err(VerificationError::Refused(RefusalReason::UnknownKey)),
                    |key| Ok(use_key(key)),
                );

                *cached = Some(Cached {
                    keys,
                    fetched_at: now,
                });

                outcome
            }

            // The refresh failed. Whatever is cached cannot answer for this id
            // — if it could, the branch above would have served it — so there
            // is nothing to fall back to and nothing to weaken.
            Err(_) => Err(VerificationError::Unavailable(if stale {
                UnavailableReason::KeysTooOld
            } else {
                UnavailableReason::KeysUnreachable
            })),
        }
    }

    /// The lock for one issuer, created on first use.
    async fn entry_for(&self, issuer: &str) -> Arc<Mutex<Option<Cached>>> {
        let mut entries = self.entries.lock().await;

        Arc::clone(
            entries
                .entry(issuer.to_owned())
                .or_insert_with(|| Arc::new(Mutex::new(None))),
        )
    }
}
