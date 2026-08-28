//! Holding an admin token for as long as it is good, and no longer.

use std::sync::Arc;
use std::time::{Duration, Instant};

use fabric_core::Clock;
use fabric_reconciliation::ProviderError;
use tokio::sync::Mutex;

use crate::admin::errors::{status_failure, transport_failure};
use crate::wire::TokenResponse;
use crate::{AdminCredential, KeycloakConfig};

/// How long before expiry a cached token is treated as spent.
///
/// A sweep can take several seconds, and a token that expires mid-sweep fails
/// the remaining clients with a 401 that looks exactly like a misconfigured
/// credential. Refreshing early costs one extra token request per sweep at
/// worst.
const EXPIRY_MARGIN: Duration = Duration::from_secs(30);

/// A token, and when it stops being usable.
struct CachedToken {
    /// The bearer value.
    value: String,

    /// When it should no longer be presented.
    good_until: Instant,
}

/// Obtains and caches the platform's admin token.
pub(super) struct TokenCache {
    /// The current token, if there is a usable one.
    current: Mutex<Option<CachedToken>>,

    /// The credential exchanged for it.
    credential: AdminCredential,

    /// The token endpoint.
    endpoint: String,

    /// The machine identity's client id.
    client_id: String,

    /// Measures the token's remaining life.
    ///
    /// Monotonic, deliberately: a token's validity is a duration, and reading
    /// it off wall-clock time would make an NTP step either expire every token
    /// at once or extend one past its actual life.
    clock: Arc<dyn Clock>,
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

    /// Exchanges the credential for a token.
    async fn request(&self, http: &reqwest::Client) -> Result<CachedToken, ProviderError> {
        let form = [
            ("grant_type", "client_credentials"),
            ("client_id", self.client_id.as_str()),
            ("client_secret", self.credential.expose()),
        ];

        let response = http
            .post(&self.endpoint)
            .form(&form)
            .send()
            .await
            .map_err(|error| transport_failure("the Keycloak token request", &error))?;

        if !response.status().is_success() {
            return Err(status_failure("the Keycloak token request", response.status()));
        }

        let token: TokenResponse = response
            .json()
            .await
            .map_err(|error| transport_failure("the Keycloak token request", &error))?;

        let life = Duration::from_secs(token.expires_in).saturating_sub(EXPIRY_MARGIN);

        Ok(CachedToken {
            value: token.access_token,
            good_until: self.clock.now() + life,
        })
    }
}
