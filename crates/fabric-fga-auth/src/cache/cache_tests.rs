//! The rotation rules, driven deterministically.
//!
//! A refresh that fails is an availability problem. Every test here exists to
//! pin the line between that and a refused credential, because the tempting
//! simplification — treat a fetch failure as "no key, so 401" — is exactly the
//! bug: it tells a legitimate user their credentials are wrong while the
//! identity provider is down.

use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use async_trait::async_trait;
use fabric_core::Clock;
use jsonwebtoken::{Algorithm, DecodingKey};

use crate::{
    IssuerRegistration, KeyCache, KeySet, KeySource, RefusalReason, UnavailableReason, VerificationError,
};

const SECRET: &[u8] = b"a-test-signing-secret";
const MAX_KEY_AGE: u64 = 43_200;

/// A clock the test moves by hand.
struct MovableClock(AtomicU64);

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
async fn a_cached_key_keeps_working_while_the_provider_is_down() {
    let source = Scripted::publishing(&["kid-1"]);
    let clock = Arc::new(MovableClock(AtomicU64::new(1_000)));
    let cache = cache_over(&source, &clock);

    ask(&cache, "kid-1").await.expect("first ask populates the cache");
    source.goes_down();

    // The whole point of caching: a provider blip must not become an outage.
    ask(&cache, "kid-1").await.expect("a cached key still serves");
    assert_eq!(source.calls(), 1, "no second fetch was needed");
}

#[tokio::test]
async fn an_unfamiliar_key_provokes_one_refresh_and_then_serves() {
    let source = Scripted::publishing(&["kid-1"]);
    let clock = Arc::new(MovableClock(AtomicU64::new(1_000)));
    let cache = cache_over(&source, &clock);

    ask(&cache, "kid-1").await.expect("populates");
    source.now_publishes(&["kid-1", "kid-2"]);
    clock.0.store(1_100, Ordering::SeqCst);

    ask(&cache, "kid-2").await.expect("the rotated key is picked up");
    assert_eq!(source.calls(), 2, "exactly one refresh");
}

#[tokio::test]
async fn a_key_the_issuer_genuinely_does_not_publish_is_refused() {
    let source = Scripted::publishing(&["kid-1"]);
    let clock = Arc::new(MovableClock(AtomicU64::new(1_000)));
    let cache = cache_over(&source, &clock);

    let error = ask(&cache, "kid-absent").await.expect_err("must refuse");

    // A key set known to be current does not publish it. That is the caller's
    // problem, and the only branch here that is a 401.
    assert_eq!(error, VerificationError::Refused(RefusalReason::UnknownKey));
}

#[tokio::test]
async fn an_unreachable_provider_is_never_reported_as_a_bad_credential() {
    let source = Scripted::publishing(&["kid-1"]);
    source.goes_down();
    let clock = Arc::new(MovableClock(AtomicU64::new(1_000)));
    let cache = cache_over(&source, &clock);

    let error = ask(&cache, "kid-1").await.expect_err("cannot establish trust");

    assert_eq!(
        error,
        VerificationError::Unavailable(UnavailableReason::KeysUnreachable),
        "a provider outage must never tell a legitimate user their token is bad"
    );
}

#[tokio::test]
async fn keys_older_than_the_bound_stop_being_trusted() {
    let source = Scripted::publishing(&["kid-1"]);
    let clock = Arc::new(MovableClock(AtomicU64::new(1_000)));
    let cache = cache_over(&source, &clock);

    ask(&cache, "kid-1").await.expect("populates");
    source.goes_down();
    clock.0.store(1_000 + MAX_KEY_AGE + 1, Ordering::SeqCst);

    let error = ask(&cache, "kid-1").await.expect_err("too old to trust");

    // The case where continuing to serve is the wrong instinct: a key removed
    // during a long outage would otherwise stay usable indefinitely.
    assert_eq!(
        error,
        VerificationError::Unavailable(UnavailableReason::KeysTooOld)
    );
}

#[tokio::test]
async fn a_flood_of_unfamiliar_key_ids_does_not_become_a_flood_of_fetches() {
    let source = Scripted::publishing(&["kid-1"]);
    let clock = Arc::new(MovableClock(AtomicU64::new(1_000)));
    let cache = cache_over(&source, &clock);

    for attempt in 0..500 {
        let _ = ask(&cache, &format!("made-up-{attempt}")).await;
    }

    // Without the minimum refresh interval this is 500 fetches aimed at the
    // identity provider, one per attacker-chosen `kid`.
    assert_eq!(
        source.calls(),
        1,
        "the first miss refreshes; the rest are refused from a set known to be current"
    );
}

#[tokio::test]
async fn a_failed_refresh_never_produces_a_key() {
    let source = Scripted::publishing(&["kid-1"]);
    let clock = Arc::new(MovableClock(AtomicU64::new(1_000)));
    let cache = cache_over(&source, &clock);

    ask(&cache, "kid-1").await.expect("populates");
    source.goes_down();
    clock.0.store(1_100, Ordering::SeqCst);

    // An id the cache has never seen, with the provider down. There is nothing
    // to fall back to, and falling back to something else would be the bug.
    let error = ask(&cache, "kid-2").await.expect_err("nothing to serve");

    assert!(matches!(error, VerificationError::Unavailable(_)));
}
