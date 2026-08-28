//! Holding an admin token for as long as it is good, and no longer.

mod exchange;

use std::sync::Arc;
use std::time::{Duration, Instant};

use fabric_core::Clock;
use fabric_reconciliation::ProviderError;
use tokio::sync::Mutex;

use crate::{AdminCredential, KeycloakConfig};

/// How long before expiry a cached token is treated as spent.
///
/// A sweep can take several seconds, and a token that expires mid-sweep fails
/// the remaining clients with a 401 that looks exactly like a misconfigured
/// credential. Refreshing early costs one extra token request per sweep at
/// worst.
pub(super) const EXPIRY_MARGIN: Duration = Duration::from_secs(30);

/// The shortest a token is ever cached for.
///
/// `master` ships with a 60-second access-token lifespan, so the margin above
/// leaves 30 seconds — fine. A realm configured tighter than the margin would
/// leave *nothing*, and the cache would re-authenticate on every single call.
/// The floor turns that pathological case into a merely imperfect one: a few
/// requests may present a token in its last seconds, and `requests` retries a
/// rejection with a fresh one.
pub(super) const MINIMUM_LIFETIME: Duration = Duration::from_secs(5);

/// A token, and when it stops being usable.
pub(super) struct CachedToken {
    /// The bearer value.
    pub(super) value: String,

    /// When it should no longer be presented.
    pub(super) good_until: Instant,
}

/// Obtains and caches the platform's admin token.
pub(super) struct TokenCache {
    /// The current token, if there is a usable one.
    current: Mutex<Option<CachedToken>>,

    /// The credential exchanged for it.
    pub(super) credential: AdminCredential,

    /// The token endpoint.
    pub(super) endpoint: String,

    /// The machine identity's client id.
    pub(super) client_id: String,

    /// Measures the token's remaining life.
    ///
    /// Monotonic, deliberately: a token's validity is a duration, and reading
    /// it off wall-clock time would make an NTP step either expire every token
    /// at once or extend one past its actual life.
    pub(super) clock: Arc<dyn Clock>,
}

impl TokenCache {
    /// Builds a cache over one deployment's token endpoint.
    pub(super) fn new(
        config: &KeycloakConfig,
        credential: AdminCredential,
        endpoint: String,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            current: Mutex::new(None),
            credential,
            endpoint,
            client_id: config.client_id.clone(),
            clock,
        }
    }

    /// Returns a usable token, obtaining one if the cached one is spent.
    ///
    /// # Errors
    ///
    /// Returns [`ProviderError`] if the token endpoint could not be reached or
    /// refused the credential.
    pub(super) async fn token(&self, http: &reqwest::Client) -> Result<String, ProviderError> {
        // Held across the request on purpose. Two concurrent sweeps would
        // otherwise both re-authenticate; the lock makes the second wait and
        // reuse the first one's token, which is what a token endpoint with a
        // rate limit expects to see.
        let mut current = self.current.lock().await;

        if let Some(cached) = current.as_ref() {
            if self.clock.now() < cached.good_until {
                return Ok(cached.value.clone());
            }
        }

        let fresh = self.request(http).await?;
        let value = fresh.value.clone();
        *current = Some(fresh);

        Ok(value)
    }

    /// Discards the cached token, so the next call mints a fresh one.
    ///
    /// # Why a valid token is ever thrown away
    ///
    /// Because a service account's *grants* can change while its token is
    /// still valid, and SaaS Fabric is what changes them. Creating a realm
    /// causes Keycloak to grant the creator that realm's administrative roles
    /// — but only into tokens minted afterwards. The token in hand was minted
    /// before the realm existed, so it authenticates fine and is refused for
    /// everything inside the realm it just created.
    ///
    /// Waiting for the cache to expire would also work, and would mean a
    /// client's first reconciliation silently failing for up to a minute for
    /// a reason no log explains.
    pub(super) async fn invalidate(&self) {
        *self.current.lock().await = None;
    }
}
