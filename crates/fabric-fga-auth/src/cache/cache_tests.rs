//! The rule these tests exist to hold, whatever the timing knobs become:
//!
//! > An unknown `kid` is refused **only** when a sufficiently fresh,
//! > successfully fetched snapshot positively establishes that the key is
//! > absent. If the verifier cannot obtain trust material fresh enough to make
//! > that claim, the answer is that it does not know.
//!
//! Two windows serve two different jobs and are deliberately not one number.
//! The tests below move the clock by the constants rather than by literals, so
//! that changing either window cannot quietly turn a security property into a
//! passing assertion about arithmetic.
//!
//! The corollary worth stating: a **failed** attempt is silent. It bounds the
//! next call and says nothing about what the issuer publishes, so it can never
//! age into evidence that a key is absent.

use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use async_trait::async_trait;
use fabric_core::Clock;
use jsonwebtoken::{Algorithm, DecodingKey};

use super::held::{REFRESH_MIN_INTERVAL_SECONDS, UNKNOWN_KID_FRESHNESS_SECONDS};

use crate::{
    IssuerRegistration, KeyCache, KeySet, KeySource, RefusalReason, UnavailableReason, VerificationError,
};

const SECRET: &[u8] = b"a-test-signing-secret";
const MAX_KEY_AGE: u64 = 43_200;

/// A clock the test moves by hand.
struct MovableClock(AtomicU64);

impl MovableClock {
    /// Moves time forward by `seconds`.
    fn advance(&self, seconds: u64) {
        self.0.fetch_add(seconds, Ordering::SeqCst);
    }
}

impl Clock for MovableClock {
    fn now(&self) -> Instant {
        Instant::now()
    }

    fn now_unix_seconds(&self) -> u64 {
        self.0.load(Ordering::SeqCst)
    }
}

/// A key source the test scripts: what it publishes, or nothing at all.
struct Scripted {
    /// The ids published, or `None` when the fetch should fail.
    published: Mutex<Option<Vec<String>>>,

    /// How many times it has been asked.
    calls: AtomicUsize,
}

impl Scripted {
    fn publishing(ids: &[&str]) -> Arc<Self> {
        Arc::new(Self {
            published: Mutex::new(Some(ids.iter().map(|id| (*id).to_owned()).collect())),
            calls: AtomicUsize::new(0),
        })
    }

    fn goes_down(&self) {
        *self.published.lock().expect("not poisoned") = None;
    }

    fn now_publishes(&self, ids: &[&str]) {
        *self.published.lock().expect("not poisoned") = Some(ids.iter().map(|id| (*id).to_owned()).collect());
    }

    fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl KeySource for Scripted {
    async fn fetch(&self, _jwks_uri: &str) -> Result<KeySet, String> {
        self.calls.fetch_add(1, Ordering::SeqCst);

        let held = self.published.lock().expect("not poisoned");

        held.as_ref().map_or_else(
            || Err("the key endpoint is unreachable".to_owned()),
            |ids| {
                Ok(KeySet::from_entries(
                    ids.iter()
                        .map(|id| (id.clone(), DecodingKey::from_secret(SECRET))),
                ))
            },
        )
    }
}

fn registration() -> IssuerRegistration {
    IssuerRegistration {
        tenant: "acme".to_owned(),
        issuer: "https://identity.example/realms/acme".to_owned(),
        audience: "workspec".to_owned(),
        jwks_uri: "https://keycloak.internal/certs".to_owned(),
        algorithms: vec![Algorithm::HS256],
        store: "01ACMESTORE".to_owned(),
        authorization_model_id: "01ACMEMODEL".to_owned(),
        max_key_age_seconds: MAX_KEY_AGE,
    }
}

/// Asks the cache for a key, reporting only whether it produced one.
async fn ask(cache: &KeyCache, key_id: &str) -> Result<(), VerificationError> {
    cache.with_key(&registration(), key_id, |_| ()).await
}

fn cache_over(source: &Arc<Scripted>, clock: &Arc<MovableClock>) -> KeyCache {
    KeyCache::new(
        Arc::clone(source) as Arc<dyn KeySource>,
        Arc::clone(clock) as Arc<dyn Clock>,
    )
}

#[tokio::test]
async fn a_usable_cached_key_keeps_working_while_the_provider_is_down() {
    let source = Scripted::publishing(&["kid-1"]);
    let clock = Arc::new(MovableClock(AtomicU64::new(1_000)));
    let cache = cache_over(&source, &clock);

    ask(&cache, "kid-1").await.expect("first ask populates the cache");
    source.goes_down();

    // The point of caching: a provider blip must not become an outage.
    ask(&cache, "kid-1").await.expect("a cached key still serves");
    assert_eq!(source.calls(), 1, "no second call was needed");
}

#[tokio::test]
async fn refusing_a_key_requires_fresh_positive_evidence_that_it_is_absent() {
    let source = Scripted::publishing(&["kid-1"]);
    let clock = Arc::new(MovableClock(AtomicU64::new(1_000)));
    let cache = cache_over(&source, &clock);

    // A successful snapshot exists and does not publish the id. That is
    // evidence, and refusing is a statement of it.
    assert_eq!(
        ask(&cache, "kid-absent").await.expect_err("must refuse"),
        VerificationError::Refused(RefusalReason::UnknownKey)
    );
}

#[tokio::test]
async fn evidence_expires_and_the_answer_follows_it_rather_than_the_token() {
    let source = Scripted::publishing(&["kid-1"]);
    let clock = Arc::new(MovableClock(AtomicU64::new(1_000)));
    let cache = cache_over(&source, &clock);

    ask(&cache, "kid-1").await.expect("populates a snapshot");

    // While the snapshot proves absence, the same id is refused.
    assert!(matches!(
        ask(&cache, "kid-absent").await,
        Err(VerificationError::Refused(_))
    ));

    // Once it no longer proves absence, and the issuer cannot be reached to
    // renew that claim, the honest answer becomes "I do not know". Same id,
    // same token, different evidence.
    source.goes_down();
    clock.advance(UNKNOWN_KID_FRESHNESS_SECONDS + 1);

    assert!(matches!(
        ask(&cache, "kid-absent").await,
        Err(VerificationError::Unavailable(_))
    ));
}

#[tokio::test]
async fn a_failed_attempt_never_becomes_evidence_about_a_key() {
    let source = Scripted::publishing(&["kid-1"]);
    let clock = Arc::new(MovableClock(AtomicU64::new(1_000)));
    let cache = cache_over(&source, &clock);

    ask(&cache, "kid-1").await.expect("populates a snapshot");
    source.goes_down();

    // Age the snapshot past proving absence, then fail a refresh.
    clock.advance(UNKNOWN_KID_FRESHNESS_SECONDS + 1);
    assert!(matches!(
        ask(&cache, "kid-absent").await,
        Err(VerificationError::Unavailable(_))
    ));

    // The failure must not have refreshed anything. Asking again inside the
    // cooldown still cannot refuse: a call we did not make, and a call that
    // failed, are both silent about what the issuer publishes.
    assert!(
        matches!(
            ask(&cache, "kid-absent").await,
            Err(VerificationError::Unavailable(_))
        ),
        "a failed attempt must not age into negative evidence"
    );
}

#[tokio::test]
async fn a_fresh_snapshot_absorbs_a_flood_without_calling_the_issuer_again() {
    let source = Scripted::publishing(&["kid-1"]);
    let clock = Arc::new(MovableClock(AtomicU64::new(1_000)));
    let cache = cache_over(&source, &clock);

    for attempt in 0..500 {
        let error = ask(&cache, &format!("made-up-{attempt}"))
            .await
            .expect_err("an invented id is not published");

        assert_eq!(
            error,
            VerificationError::Refused(RefusalReason::UnknownKey),
            "each is answered from the same fresh snapshot"
        );
    }

    assert_eq!(
        source.calls(),
        1,
        "the first miss fetches; the rest are answered from that generation"
    );
}

#[tokio::test]
async fn an_outage_bounds_calls_too_rather_than_retrying_every_request() {
    let source = Scripted::publishing(&["kid-1"]);
    source.goes_down();
    let clock = Arc::new(MovableClock(AtomicU64::new(1_000)));
    let cache = cache_over(&source, &clock);

    // No snapshot at all and the issuer unreachable. Every one of these must
    // answer unavailable, and they must not each call a provider that is
    // already unwell.
    for attempt in 0..500 {
        assert!(
            matches!(
                ask(&cache, &format!("made-up-{attempt}")).await,
                Err(VerificationError::Unavailable(_))
            ),
            "nothing is known, so nothing may be refused"
        );
    }

    assert_eq!(source.calls(), 1, "the cooldown bounds failing calls as well");

    // Once the cooldown elapses exactly one more attempt is permitted.
    clock.advance(REFRESH_MIN_INTERVAL_SECONDS);
    let _ = ask(&cache, "made-up-again").await;
    assert_eq!(source.calls(), 2);
}

#[tokio::test]
async fn a_recovered_provider_is_believed_again() {
    let source = Scripted::publishing(&["kid-1"]);
    source.goes_down();
    let clock = Arc::new(MovableClock(AtomicU64::new(1_000)));
    let cache = cache_over(&source, &clock);

    assert!(matches!(
        ask(&cache, "kid-1").await,
        Err(VerificationError::Unavailable(_))
    ));

    source.now_publishes(&["kid-1", "kid-2"]);
    clock.advance(REFRESH_MIN_INTERVAL_SECONDS);

    ask(&cache, "kid-2").await.expect("a rotated key is picked up");
}

#[tokio::test]
async fn keys_older_than_the_bound_stop_being_trusted() {
    let source = Scripted::publishing(&["kid-1"]);
    let clock = Arc::new(MovableClock(AtomicU64::new(1_000)));
    let cache = cache_over(&source, &clock);

    ask(&cache, "kid-1").await.expect("populates");
    source.goes_down();
    clock.advance(MAX_KEY_AGE + 1);

    // The case where continuing to serve is the wrong instinct: a key removed
    // during a long outage would otherwise stay usable indefinitely.
    assert_eq!(
        ask(&cache, "kid-1").await.expect_err("too old to trust"),
        VerificationError::Unavailable(UnavailableReason::KeysTooOld)
    );
}
