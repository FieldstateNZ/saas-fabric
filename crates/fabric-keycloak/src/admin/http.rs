//! The admin HTTP client.

use std::sync::Arc;
use std::time::Duration;

use fabric_core::Clock;
use fabric_reconciliation::ProviderError;

use crate::admin::errors::transport_failure;
use crate::admin::token::TokenCache;
use crate::admin::Paths;
use crate::{AdminCredential, KeycloakConfig};
use reqwest::StatusCode;

/// Issues authenticated requests against Keycloak's admin API.
pub(crate) struct KeycloakAdmin {
    /// The HTTP client, which owns the keep-alive connection pool.
    pub(super) http: reqwest::Client,

    /// The admin token, cached for as long as it is good.
    tokens: TokenCache,

    /// Where each resource lives.
    paths: Paths,
}

impl KeycloakAdmin {
    /// Builds a client from configuration and a resolved credential.
    ///
    /// # Errors
    ///
    /// Returns a message if the configuration is invalid or the HTTP client
    /// cannot be constructed.
    pub(crate) fn new(
        config: &KeycloakConfig,
        credential: AdminCredential,
        clock: Arc<dyn Clock>,
    ) -> Result<Self, String> {
        config.validate()?;

        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(config.http_timeout_seconds))
            .build()
            .map_err(|error| format!("keycloak: could not build an HTTP client: {error}"))?;

        let paths = Paths::new(&config.base_url);
        let tokens = TokenCache::new(config, credential, paths.token(&config.admin_realm), clock);

        Ok(Self { http, tokens, paths })
    }

    /// Where each resource lives.
    pub(crate) const fn paths(&self) -> &Paths {
        &self.paths
    }

    /// Attaches the admin token and sends the request, retrying once if the
    /// token is refused.
    ///
    /// # Why a refusal is retried at all
    ///
    /// A service account's grants can change while its token is still valid,
    /// and SaaS Fabric is what changes them: creating a realm causes Keycloak
    /// to grant the creator that realm's administrative roles, into tokens
    /// minted *afterwards*. So the first pass over a new client creates the
    /// realm with the token it holds and is then refused for everything inside
    /// it — with a token that is perfectly valid and simply too old to know
    /// about the realm.
    ///
    /// Retrying once with a fresh token turns that into a single extra round
    /// trip on the one pass where it matters. It cannot loop: the retry is not
    /// itself retried, so a genuine permissions problem still fails, one
    /// request later and with the same error.
    ///
    /// The request is cloned before the first attempt. Every body this crate
    /// sends is in-memory JSON, so `try_clone` succeeds; if it ever did not,
    /// the first attempt's result stands rather than the call failing for a
    /// reason unrelated to Keycloak.
    pub(super) async fn send(
        &self,
        operation: &str,
        request: reqwest::RequestBuilder,
    ) -> Result<reqwest::Response, ProviderError> {
        let retry = request.try_clone();
        let token = self.tokens.token(&self.http).await?;

        let response = request
            .bearer_auth(token)
            .send()
            .await
            .map_err(|error| transport_failure(operation, &error))?;

        if !was_refused(response.status()) {
            return Ok(response);
        }

        let Some(retry) = retry else {
            return Ok(response);
        };

        self.tokens.invalidate().await;
        let fresh = self.tokens.token(&self.http).await?;

        retry
            .bearer_auth(fresh)
            .send()
            .await
            .map_err(|error| transport_failure(operation, &error))
    }
}

/// Whether a status means the token was refused rather than the request.
///
/// Both are worth retrying with a fresh token, for different reasons: `401`
/// means the token is not accepted at all, and `403` means it is accepted and
/// does not carry the grant — which is the case that arises when SaaS Fabric
/// has just changed its own grants.
fn was_refused(status: StatusCode) -> bool {
    status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN
}
