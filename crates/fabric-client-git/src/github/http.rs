//! The contents-API client.

use fabric_client_model::ClientId;
use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, USER_AGENT};

use crate::{GitCredential, GitRepositoryConfig};

/// The API version this adapter is written against.
///
/// Pinned in a header rather than assumed, so a provider-side default moving
/// on does not silently change what these calls mean.
const API_VERSION_HEADER: &str = "X-GitHub-Api-Version";

/// The version pinned above.
const API_VERSION: &str = "2022-11-28";

/// Issues authenticated requests against one repository's contents API.
pub(crate) struct GitHost {
    /// The HTTP client, which owns the keep-alive connection pool.
    pub(super) http: reqwest::Client,

    /// The credential presented on every request.
    credential: GitCredential,

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
    pub(crate) fn new(config: &GitRepositoryConfig, credential: GitCredential) -> Result<Self, String> {
        config.validate()?;

        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(config.http_timeout_seconds))
            .build()
            .map_err(|error| format!("clients repository: could not build an HTTP client: {error}"))?;

        Ok(Self {
            http,
            credential,
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

    /// Applies the headers every request needs.
    pub(super) fn request(&self, builder: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        let mut headers = HeaderMap::new();
        headers.insert(ACCEPT, HeaderValue::from_static("application/vnd.github+json"));
        headers.insert(API_VERSION_HEADER, HeaderValue::from_static(API_VERSION));
        // Required by the host, which refuses requests without one.
        headers.insert(USER_AGENT, HeaderValue::from_static("saas-fabric-control-plane"));

        builder.headers(headers).bearer_auth(self.credential.expose())
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
