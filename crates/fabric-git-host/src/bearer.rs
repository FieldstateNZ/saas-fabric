//! Obtaining the bearer the host's API is called with.

use std::sync::Arc;
use std::time::Instant;

use fabric_core::Clock;
use tokio::sync::Mutex;

mod assertion;
mod lifetime;
mod wire;

pub use assertion::sign_app_assertion;

use crate::errors::{status_failure, transport_failure};
use crate::{GitCredential, TokenError};
use wire::InstallationToken;

/// A minted token, and when it stops being used.
struct Cached {
    /// The bearer value.
    value: String,

    /// When it should no longer be presented.
    good_until: Instant,
}

/// Supplies the bearer for each request, minting one when the posture needs it.
pub struct BearerSource {
    /// What the platform was given.
    credential: GitCredential,

    /// The current installation token, for the App posture.
    cached: Mutex<Option<Cached>>,

    /// Where the token endpoint lives.
    api_base_url: String,

    /// Measures token lifetime, and stamps the JWT.
    clock: Arc<dyn Clock>,
}

impl BearerSource {
    /// Builds a source over a credential.
    #[must_use]
    pub fn new(credential: GitCredential, api_base_url: String, clock: Arc<dyn Clock>) -> Self {
        Self {
            credential,
            cached: Mutex::new(None),
            api_base_url,
            clock,
        }
    }

    /// Returns a bearer for the next request.
    ///
    /// # Errors
    ///
    /// Returns [`TokenError`] if an installation token could not be minted —
    /// the key was refused, or the token endpoint was unreachable.
    pub async fn bearer(&self, http: &reqwest::Client) -> Result<String, TokenError> {
        let (app_id, installation_id, private_key) = match &self.credential {
            GitCredential::Token(value) => return Ok(value.clone()),
            GitCredential::App {
                app_id,
                installation_id,
                private_key,
            } => (app_id, installation_id, private_key),
        };

        // Held across the mint on purpose. Two concurrent sweeps would
        // otherwise both exchange the key; the lock makes the second wait and
        // reuse the first one's token.
        let mut cached = self.cached.lock().await;

        if let Some(current) = cached.as_ref() {
            if self.clock.now() < current.good_until {
                return Ok(current.value.clone());
            }
        }

        let assertion = sign_app_assertion(app_id, private_key, self.clock.now_unix_seconds())?;
        let minted = self.mint(http, installation_id, &assertion).await?;

        // The host's stated expiry, measured monotonically — see
        // `lifetime::usable_for` for why the two clocks are mixed on purpose.
        let usable_for = lifetime::usable_for(&minted.expires_at, self.clock.now_unix_seconds());
        let value = minted.token.clone();

        *cached = Some(Cached {
            value: minted.token,
            good_until: self.clock.now() + usable_for,
        });

        Ok(value)
    }

    /// Discards the cached token, so the next request mints a fresh one.
    ///
    /// # Why a token inside its stated lifetime is ever thrown away
    ///
    /// Because a stated expiry is not a guarantee that the token still works.
    /// An App can be uninstalled, its key rotated, or its installation
    /// suspended — and every one of those invalidates a token the platform
    /// believes it may use for another forty minutes.
    ///
    /// Without this, a token that stopped working would be presented on every
    /// request until the local deadline passed, and every call in between
    /// would fail identically. Callers invalidate on a `401`.
    pub async fn invalidate(&self) {
        *self.cached.lock().await = None;
    }

    /// Exchanges the assertion for an installation token.
    async fn mint(
        &self,
        http: &reqwest::Client,
        installation_id: &str,
        assertion: &str,
    ) -> Result<InstallationToken, TokenError> {
        let url = format!(
            "{}/app/installations/{installation_id}/access_tokens",
            self.api_base_url.trim_end_matches('/')
        );

        let response = http
            .post(url)
            .header(reqwest::header::ACCEPT, "application/vnd.github+json")
            .header(reqwest::header::USER_AGENT, "saas-fabric-control-plane")
            .bearer_auth(assertion)
            .send()
            .await
            .map_err(|error| transport_failure(&error))?;

        if !response.status().is_success() {
            return Err(status_failure(response.status(), response.headers()));
        }

        response.json().await.map_err(|error| transport_failure(&error))
    }
}
