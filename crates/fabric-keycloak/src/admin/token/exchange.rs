//! Exchanging the platform's credential for an admin token.
//!
//! Split from the cache because they are different concerns: the cache decides
//! *when* a token is obtained, and this decides *how*.

use std::time::Duration;

use fabric_reconciliation::ProviderError;

use crate::admin::errors::{status_failure, transport_failure};
use crate::admin::token::{CachedToken, TokenCache, EXPIRY_MARGIN, MINIMUM_LIFETIME};
use crate::wire::TokenResponse;

impl TokenCache {
    /// Exchanges the credential for a token.
    pub(super) async fn request(&self, http: &reqwest::Client) -> Result<CachedToken, ProviderError> {
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

        let life = Duration::from_secs(token.expires_in)
            .saturating_sub(EXPIRY_MARGIN)
            .max(MINIMUM_LIFETIME);

        Ok(CachedToken {
            value: token.access_token,
            good_until: self.clock.now() + life,
        })
    }
}
