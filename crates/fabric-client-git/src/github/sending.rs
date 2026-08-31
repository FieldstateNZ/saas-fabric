//! Presenting a bearer, and replacing one the host rejects.
//!
//! Split from the client's own file because it is a policy rather than a
//! construction detail: *when* a minted token is discarded, and how many times
//! a rejected request is retried.

use fabric_control_plane::RepositoryError;
use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, USER_AGENT};
use reqwest::StatusCode;

use crate::github::errors::{token_failure, transport_failure};
use crate::github::http::{GitHost, API_VERSION, API_VERSION_HEADER};

impl GitHost {
    /// Sends a request with a current bearer, re-minting once if it is
    /// rejected.
    ///
    /// # Why a rejection is retried
    ///
    /// A minted token's stated expiry is not a guarantee that it still works.
    /// An App can be uninstalled, its key rotated, or its installation
    /// suspended, and each of those kills a token the platform believes it may
    /// use for another forty minutes. Without a retry the platform would
    /// present the dead token on every request until its local deadline
    /// passed, and every sweep in between would fail identically — which is a
    /// long time to be broken for a condition one extra request detects.
    ///
    /// # Why only `401`, where the Keycloak adapter also retries `403`
    ///
    /// The two hosts mean different things by it. Keycloak answers `403` when
    /// a token is authentic but predates a grant — a state SaaS Fabric creates
    /// for itself by creating a realm, so a fresh token fixes it. GitHub
    /// answers `403` for rate limiting and for permissions the installation
    /// genuinely does not have, and a fresh token fixes neither. Retrying it
    /// here would spend a mint on every rate-limited request.
    ///
    /// # Errors
    ///
    /// Returns [`RepositoryError`] if a bearer could not be obtained or the
    /// request could not be sent.
    pub(super) async fn send(
        &self,
        operation: &str,
        builder: reqwest::RequestBuilder,
    ) -> Result<reqwest::Response, RepositoryError> {
        // Cloned before the first attempt. Every body this crate sends is
        // in-memory JSON, so `try_clone` succeeds; if it ever did not, the
        // first attempt's result stands rather than the call failing for a
        // reason unrelated to the host.
        let retry = builder.try_clone();

        let response = self.attempt(operation, builder).await?;

        if response.status() != StatusCode::UNAUTHORIZED {
            return Ok(response);
        }

        let Some(retry) = retry else {
            return Ok(response);
        };

        self.bearers.invalidate().await;

        self.attempt(operation, retry).await
    }

    /// Applies the headers every request needs, including a current bearer,
    /// and sends it once.
    async fn attempt(
        &self,
        operation: &str,
        builder: reqwest::RequestBuilder,
    ) -> Result<reqwest::Response, RepositoryError> {
        let mut headers = HeaderMap::new();
        headers.insert(ACCEPT, HeaderValue::from_static("application/vnd.github+json"));
        headers.insert(API_VERSION_HEADER, HeaderValue::from_static(API_VERSION));
        // Required by the host, which refuses requests without one.
        headers.insert(USER_AGENT, HeaderValue::from_static("saas-fabric-control-plane"));

        let bearer = self.bearers.bearer(&self.http).await.map_err(token_failure)?;

        builder
            .headers(headers)
            .bearer_auth(bearer)
            .send()
            .await
            .map_err(|error| transport_failure(operation, &error))
    }
}
