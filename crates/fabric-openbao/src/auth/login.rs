//! The login exchange itself.
//!
//! Split from the cache because they are different concerns: the cache decides
//! *when* a token is obtained, and this decides *how* — the same split the
//! Keycloak adapter makes for the same reason.

use std::time::Duration;

use serde::Deserialize;

use crate::auth::{Held, TokenCache, EXPIRY_MARGIN, MINIMUM_LIFETIME};

/// What a successful login returns.
#[derive(Deserialize)]
struct LoginResponse {
    /// The authentication block.
    auth: Auth,
}

/// The part of a login this platform reads.
#[derive(Deserialize)]
struct Auth {
    /// The token to present on subsequent requests.
    client_token: String,

    /// How long it is good for, in seconds.
    lease_duration: u64,
}

impl TokenCache {
    /// Exchanges the pod's own identity for a store token.
    pub(super) async fn login(&self, http: &reqwest::Client) -> Result<Held, String> {
        let jwt = tokio::fs::read_to_string(&self.token_path).await.map_err(|_| {
            format!(
                "the service-account token at {} could not be read",
                self.token_path
            )
        })?;

        let response = http
            .post(&self.endpoint)
            .json(&serde_json::json!({ "role": self.role, "jwt": jwt.trim() }))
            .send()
            .await
            .map_err(|_| "the secret store could not be reached to log in".to_owned())?;

        if !response.status().is_success() {
            // The body is deliberately not read. A failed login's body can
            // echo the assertion that was presented, and that assertion is a
            // credential.
            return Err(format!(
                "the secret store refused the login ({})",
                response.status()
            ));
        }

        let login: LoginResponse = response
            .json()
            .await
            .map_err(|_| "the secret store's login answer could not be read".to_owned())?;

        let life = Duration::from_secs(login.auth.lease_duration)
            .saturating_sub(EXPIRY_MARGIN)
            .max(MINIMUM_LIFETIME);

        Ok(Held {
            value: login.auth.client_token,
            good_until: self.clock.now() + life,
        })
    }
}
