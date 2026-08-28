//! The admin HTTP client.

use std::sync::Arc;
use std::time::Duration;

use fabric_core::Clock;
use fabric_reconciliation::ProviderError;

use crate::admin::errors::transport_failure;
use crate::admin::token::TokenCache;
use crate::admin::Paths;
use crate::{AdminCredential, KeycloakConfig};

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

    /// Attaches the admin token and sends the request.
    pub(super) async fn send(
        &self,
        operation: &str,
        request: reqwest::RequestBuilder,
    ) -> Result<reqwest::Response, ProviderError> {
        let token = self.tokens.token(&self.http).await?;

        request
            .bearer_auth(token)
            .send()
            .await
            .map_err(|error| transport_failure(operation, &error))
    }
}
