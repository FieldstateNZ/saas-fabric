//! The admin HTTP client.

use std::time::Duration;

use fabric_reconciliation::ProviderError;

use crate::admin::errors::transport_failure;
use crate::admin::Paths;
use crate::KeycloakConfig;

/// Issues authenticated requests against Keycloak's admin API.
pub(crate) struct KeycloakAdmin {
    /// The HTTP client, which owns the keep-alive connection pool.
    pub(super) http: reqwest::Client,

    /// The operator's bearer, presented on every request.
    ///
    /// Held rather than minted: this client acts with authority somebody lent
    /// it, and it has no way to obtain more. A client is built per operation
    /// for the same reason (ADR 0012).
    authority: String,

    /// Where each resource lives.
    paths: Paths,
}

impl KeycloakAdmin {
    /// Builds a client that acts with the authority it is given.
    ///
    /// # Errors
    ///
    /// Returns a message if the configuration is invalid or the HTTP client
    /// cannot be constructed.
    pub(crate) fn new(config: &KeycloakConfig, authority: &str) -> Result<Self, String> {
        config.validate()?;

        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(config.http_timeout_seconds))
            .build()
            .map_err(|error| format!("keycloak: could not build an HTTP client: {error}"))?;

        Ok(Self {
            http,
            authority: authority.to_owned(),
            paths: Paths::new(&config.base_url),
        })
    }

    /// Where each resource lives.
    pub(crate) const fn paths(&self) -> &Paths {
        &self.paths
    }

    /// Presents the operator's authority and sends the request.
    ///
    /// # There is no retry here any more, and its absence is the design
    ///
    /// There used to be one, for a real reason: creating a realm causes
    /// Keycloak to grant the creator that realm's administrative roles, and
    /// those land only in tokens minted *afterwards*. A service account could
    /// simply mint a fresh one and carry on.
    ///
    /// This client cannot. It presents authority a human lent it and has no
    /// way to obtain more, so a refusal is reported rather than worked around.
    /// The consequence is a requirement on the operator rather than on this
    /// code: their authority has to already cover realms that do not exist
    /// yet — master-realm `admin` does, `create-realm` alone does not, and the
    /// difference shows up as a `403` on the first role inside a realm the
    /// same operator has just created.
    ///
    /// That is stated in ADR 0012 and in the deployment's README, because it
    /// is the sort of thing that is discovered at three in the morning
    /// otherwise.
    pub(super) async fn send(
        &self,
        operation: &str,
        request: reqwest::RequestBuilder,
    ) -> Result<reqwest::Response, ProviderError> {
        request
            .bearer_auth(&self.authority)
            .send()
            .await
            .map_err(|error| transport_failure(operation, &error))
    }
}
