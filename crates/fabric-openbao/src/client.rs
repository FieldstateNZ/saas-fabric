//! Talking to OpenBao, and where this instance's partition is.

use std::sync::Arc;
use std::time::Duration;

use fabric_core::Clock;

use crate::auth::TokenCache;
use crate::OpenBaoConfig;

/// A client for one instance's partition of one store.
pub struct OpenBao {
    /// The API address, without a trailing slash.
    address: String,

    /// The key-value mount.
    mount: String,

    /// This instance's prefix within it.
    ///
    /// Every name is resolved beneath this, and no caller supplies a prefix —
    /// which is what makes the partition a partition rather than a convention.
    prefix: String,

    /// The store token, and how it is obtained.
    tokens: TokenCache,

    /// The HTTP client.
    http: reqwest::Client,
}

impl OpenBao {
    /// Builds a client.
    ///
    /// # Errors
    ///
    /// Returns a message if the HTTP client cannot be built.
    pub fn new(config: &OpenBaoConfig, clock: Arc<dyn Clock>) -> Result<Self, String> {
        Ok(Self {
            address: config.address.trim_end_matches('/').to_owned(),
            mount: config.mount.trim_matches('/').to_owned(),
            prefix: config.prefix.trim_matches('/').to_owned(),
            tokens: TokenCache::new(
                &config.address,
                &config.auth_mount,
                &config.role,
                &config.service_account_token_path,
                clock,
            ),
            http: reqwest::Client::builder()
                .timeout(Duration::from_secs(config.http_timeout_seconds))
                .build()
                .map_err(|error| format!("secret store: {error}"))?,
        })
    }

    /// How this store is described in the startup log.
    ///
    /// Names the address, the mount and the partition, and no credential — the
    /// token is obtained from the pod's own identity and never written down.
    #[must_use]
    pub fn describe(&self) -> String {
        format!("openbao at {} ({}/{})", self.address, self.mount, self.prefix)
    }

    /// Where an entry's data lives.
    pub(crate) fn data_url(&self, name: &str) -> String {
        format!("{}/v1/{}/data/{}/{name}", self.address, self.mount, self.prefix)
    }

    /// Where an entry's metadata lives.
    pub(crate) fn metadata_url(&self, name: &str) -> String {
        format!(
            "{}/v1/{}/metadata/{}/{name}",
            self.address, self.mount, self.prefix
        )
    }

    /// Sends a request, logging in again once if the token was refused.
    ///
    /// The retry is not politeness. A store token can be revoked before its
    /// lease expires, and the platform learns that as a `403` on an ordinary
    /// call — the same shape of failure the Keycloak adapter handles the same
    /// way, and for the same reason.
    pub(crate) async fn send(
        &self,
        method: reqwest::Method,
        url: &str,
        body: Option<serde_json::Value>,
    ) -> Result<reqwest::Response, String> {
        let first = self.attempt(method.clone(), url, body.clone()).await?;

        if first.status() != reqwest::StatusCode::FORBIDDEN {
            return Ok(first);
        }

        self.tokens.invalidate().await;
        self.attempt(method, url, body).await
    }

    /// One attempt, carrying whatever token is current.
    async fn attempt(
        &self,
        method: reqwest::Method,
        url: &str,
        body: Option<serde_json::Value>,
    ) -> Result<reqwest::Response, String> {
        let token = self.tokens.token(&self.http).await?;

        let mut request = self.http.request(method, url).header("X-Vault-Token", token);

        if let Some(body) = body {
            request = request.json(&body);
        }

        request
            .send()
            .await
            .map_err(|_| "the secret store could not be reached".to_owned())
    }
}
