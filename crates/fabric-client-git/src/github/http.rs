//! The contents-API client.

use std::sync::Arc;

use fabric_client_model::ClientId;
use fabric_core::Clock;

use fabric_git_host::{BearerSource, GitCredential};

use crate::GitRepositoryConfig;

/// The API version this adapter is written against.
///
/// Pinned in a header rather than assumed, so a provider-side default moving
/// on does not silently change what these calls mean.
pub(super) const API_VERSION_HEADER: &str = "X-GitHub-Api-Version";

/// The version pinned above.
pub(super) const API_VERSION: &str = "2022-11-28";

/// Issues authenticated requests against one repository's contents API.
pub(crate) struct GitHost {
    /// The HTTP client, which owns the keep-alive connection pool.
    pub(super) http: reqwest::Client,

    /// Supplies the bearer for each request.
    ///
    /// Not a stored token: under the App posture the bearer is minted and
    /// expires, so every request asks rather than holding one.
    pub(super) bearers: BearerSource,

    /// Where the repository is.
    pub(super) config: GitRepositoryConfig,
}

impl GitHost {
    /// Builds a client from configuration and a resolved credential.
    ///
    /// # Errors
    ///
    /// Returns a message if the configuration is invalid or the HTTP client
    /// cannot be constructed.
    pub(crate) fn new(
        config: &GitRepositoryConfig,
        credential: GitCredential,
        clock: Arc<dyn Clock>,
    ) -> Result<Self, String> {
        config.validate()?;

        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(config.http_timeout_seconds))
            .build()
            .map_err(|error| format!("clients repository: could not build an HTTP client: {error}"))?;

        Ok(Self {
            http,
            bearers: BearerSource::new(credential, config.api_base_url.clone(), clock),
            config: config.clone(),
        })
    }

    /// A description safe to log: the repository and branch, no credential.
    pub(crate) fn describe(&self) -> String {
        format!(
            "{}/{} on {}",
            self.config.owner, self.config.repository, self.config.branch
        )
    }

    /// The contents-API URL for a path within the repository.
    pub(super) fn contents_url(&self, path: &str) -> String {
        format!(
            "{}/repos/{}/{}/contents/{path}?ref={}",
            self.config.api_base_url.trim_end_matches('/'),
            self.config.owner,
            self.config.repository,
            self.config.branch
        )
    }

    /// Where a client's document lives.
    ///
    /// `client` is a validated DNS label, so it cannot contain a slash or a
    /// `..` and cannot address a path outside the clients directory. That
    /// check happens at parse time precisely so this interpolation is safe.
    pub(super) fn document_path(&self, client: &ClientId) -> String {
        format!(
            "{}/{client}/{}",
            self.config.path_prefix, self.config.document_file
        )
    }
}
