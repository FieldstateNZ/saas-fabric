//! Re-reading the identity provider's signing keys for as long as the
//! process runs, without letting an adapter panic end the schedule.
//!
//! Where the loop reads the keys from, and why that is a trait
//! ([`SigningKeySource`]) rather than `RealmSignIn` directly, is
//! [`source`]'s module doc, not this one.
//!
//! # Surviving a panic
//!
//! Each tick runs through
//! [`tick::survive_panic`](crate::startup::tick::survive_panic), in a task of
//! its own: a panic unwinding out of the adapter -- an HTTP client reacting
//! badly to a malformed response, say -- is caught there as a join failure
//! rather than ending this loop, logged as a fixed sentence without its
//! payload, and the next refresh proceeds on schedule regardless. This is the
//! same guarantee `start_sweeping` and `start_publishing` give their loops,
//! and by the same mechanism.

use std::sync::Arc;
use std::time::Duration;

use fabric_control_plane::{KeyHolder, VerificationKeys};
use tracing::Instrument;

use crate::startup::tick;

mod source;

pub(super) use source::SigningKeySource;

/// Re-reads the key set for as long as the process runs.
///
/// The first read happens immediately, before the first sleep, so a healthy
/// deployment is serving operators within a moment of starting rather than
/// after one whole interval.
///
/// Each tick runs through [`tick::survive_panic`], instrumented with the same
/// span twice: once around `survive_panic` itself, so its panic line carries
/// `issuer`, and once around the `refresh_once` future inside it, so
/// `refresh_once`'s own two log lines -- which run in the task
/// `survive_panic` spawns, not this one -- carry `issuer` too.
pub(super) fn spawn(
    source: Arc<dyn SigningKeySource>,
    keys: Arc<KeyHolder>,
    interval: Duration,
    issuer: String,
) {
    tokio::spawn(async move {
        loop {
            let source = Arc::clone(&source);
            let keys = Arc::clone(&keys);
            // `survive_panic`'s panic event is emitted in this loop task,
            // not in the task it spawns for `work` -- so the span has to
            // wrap `survive_panic(...)` as well as the block passed to it:
            // the outer `.instrument(span)` covers the panic line, the inner
            // `.instrument(span.clone())` covers the two lines
            // `refresh_once` logs, which run inside the spawned task and
            // would otherwise carry no `issuer`.
            let span = tracing::info_span!("operator_key_refresh", issuer = %issuer);
            tick::survive_panic(
                "operator key refresh",
                async move { refresh_once(&source, &keys).await }.instrument(span.clone()),
            )
            // The outer half of the pair -- see above.
            .instrument(span)
            .await;

            tokio::time::sleep(interval).await;
        }
    });
}

/// One refresh pass, with its outcome logged.
async fn refresh_once(source: &Arc<dyn SigningKeySource>, keys: &Arc<KeyHolder>) {
    match source
        .read_signing_keys()
        .await
        .and_then(|document| VerificationKeys::parse(&document))
    {
        Ok(read) => {
            tracing::info!(
                event = "control_plane.operator_keys_refreshed",
                keys = read.len(),
                "read the identity provider's signing keys"
            );
            keys.replace(read);
        }

        // Warn and keep the keys already held. A provider that is
        // briefly unreachable should not sign every operator out; the
        // keys in hand stay valid until they rotate.
        Err(error) => tracing::warn!(
            event = "control_plane.operator_keys_unavailable",
            error = %error,
            "could not read the identity provider's signing keys; keeping the set in hand"
        ),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use async_trait::async_trait;

    use super::*;

    /// A signing-key source whose `read_signing_keys` panics on its first
    /// call and counts every call, proving a panicking refresh tick does not
    /// end the schedule -- the same shape as `sweeping.rs`'s
    /// `PanicsOnceThenCounts` and `publishing.rs`'s `Untouched`.
    ///
    /// `fetch_add` runs, and is recorded, before the `assert!` decides
    /// whether to panic -- the call that panics is still counted, which is
    /// what lets the test below tell "died after the first call" from
    /// "never called again" by the count alone. Every call after the first
    /// returns `Ok`; what `VerificationKeys::parse` makes of that string
    /// does not matter here -- an `Err` from parsing it would just take the
    /// warn-and-keep path, which is also fine.
    struct PanicsOnceThenCounts {
        calls: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl SigningKeySource for PanicsOnceThenCounts {
        async fn read_signing_keys(&self) -> Result<String, String> {
            let previous = self.calls.fetch_add(1, Ordering::SeqCst);
            assert!(previous != 0, "simulated adapter failure");
            Ok("{}".to_owned())
        }
    }

    #[tokio::test(start_paused = true)]
    async fn a_panicking_read_does_not_stop_refreshing() {
        let calls = Arc::new(AtomicUsize::new(0));
        let source: Arc<dyn SigningKeySource> = Arc::new(PanicsOnceThenCounts {
            calls: Arc::clone(&calls),
        });
        let keys = KeyHolder::empty();

        spawn(
            source,
            keys,
            Duration::from_secs(1),
            "https://auth.example.test/realms/master".to_owned(),
        );

        // The immediate first tick panics; advancing past two more
        // one-second intervals must still reach `read_signing_keys()`
        // again. Yielding between advances gives the spawned loop task, and
        // the per-tick task `survive_panic` spawns inside it, a chance to
        // actually run under paused time -- nothing here is driven by a
        // real clock.
        //
        // The spawned tick's panic prints a backtrace to stderr even though
        // this test passes -- that is `tokio::spawn` reporting the panic the
        // way it always does, not a sign anything here needs fixing.
        for _ in 0..3 {
            tokio::time::advance(Duration::from_secs(1)).await;
            for _ in 0..10 {
                tokio::task::yield_now().await;
            }
        }

        let seen = calls.load(Ordering::SeqCst);
        assert!(
            seen >= 2,
            "a loop that died on the first panic would leave this at 1; saw {seen}"
        );
    }

    // No `an_unreadable_document_keeps_the_keys_in_hand` test here: proving
    // the held set is unchanged after a failed refresh needs some way to
    // read back what a `KeyHolder` currently holds from outside
    // `fabric_control_plane::operator::oidc`, and there isn't one --
    // `KeyHolder::current` is `pub(super)` to that module rather than `pub`,
    // and `VerificationKeys::held` (the constructor that crate's own tests
    // use to seed a holder without a real JWKS document) is `pub(super)`
    // there too. Adding a public accessor to `fabric-control-plane` so this
    // crate could observe it is exactly the new public API this change was
    // told not to add, so the warn-and-keep branch is exercised by
    // `refresh_once`'s own logic above and by
    // `a_panicking_read_does_not_stop_refreshing`'s use of it on every
    // non-panicking call, rather than by a dedicated test.
}
