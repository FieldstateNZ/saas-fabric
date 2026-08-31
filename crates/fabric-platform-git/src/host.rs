//! The Git Data client for one repository.

use std::sync::Arc;

use fabric_core::Clock;
use fabric_git_host::{BearerSource, GitCredential};

mod failures;
mod objects;
mod reads;
mod refs;
mod sending;
mod wire;

pub(crate) use refs::RefUpdate;

use crate::PlatformRepositoryConfig;

/// The API version this adapter is written against.
///
/// Pinned in a header rather than assumed, so a provider-side default moving
/// on does not silently change what these calls mean.
pub(crate) const API_VERSION_HEADER: &str = "X-GitHub-Api-Version";

/// The version pinned above.
pub(crate) const API_VERSION: &str = "2022-11-28";

/// Reads and atomically writes desired state in the platform repository.
pub struct PlatformGitRepository {
    /// The HTTP client, which owns the keep-alive connection pool.
    pub(crate) http: reqwest::Client,

    /// Supplies the bearer for each request.
    ///
    /// Not a stored token: under the App posture the bearer is minted and
    /// expires, so every request asks rather than holding one.
    pub(crate) bearers: BearerSource,

    /// Where the repository is.
    pub(crate) config: PlatformRepositoryConfig,
}

impl PlatformGitRepository {
    /// Builds a client from configuration and a resolved credential.
    ///
    /// # Errors
    ///
    /// Returns a message if the configuration is invalid or the HTTP client
    /// cannot be constructed. The message names a field, never its value.
    pub fn new(
        config: &PlatformRepositoryConfig,
        credential: GitCredential,
        clock: Arc<dyn Clock>,
    ) -> Result<Self, String> {
        config.validate()?;

        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(config.http_timeout_seconds))
            .build()
            .map_err(|error| format!("platform repository: could not build an HTTP client: {error}"))?;

        Ok(Self {
            http,
            bearers: BearerSource::new(credential, config.api_base_url.clone(), clock),
            config: config.clone(),
        })
    }

    /// Names the repository and branch, for a log line or a health report.
    ///
    /// No credential, and no API base URL: an Enterprise host's address is not
    /// something to scatter through logs.
    #[must_use]
    pub fn describe(&self) -> String {
        format!("{} on {}", self.config.slug(), self.config.branch)
    }

    /// A URL under this repository.
    pub(crate) fn url(&self, suffix: &str) -> String {
        format!(
            "{}/repos/{}/{suffix}",
            self.config.api_base_url.trim_end_matches('/'),
            self.config.slug()
        )
    }
}
