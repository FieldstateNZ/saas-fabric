//! How a repository's name becomes a path on this registry.

use crate::client::OciRegistry;

impl OciRegistry {
    /// How this registry names its repositories: `docker.io`, `ghcr.io`,
    /// `host:port`. The key a [`Registries`](crate::Registries) routes by.
    #[must_use]
    pub fn naming_host(&self) -> &str {
        &self.registry_host
    }

    /// The base URL for a repository's API.
    pub(crate) fn url(&self, repository: &str, suffix: &str) -> String {
        format!("{}/v2/{}/{suffix}", self.base_url, self.path(repository))
    }

    /// A repository reference with any registry host stripped off.
    ///
    /// Callers name repositories as they appear in a manifest —
    /// `ghcr.io/fieldstatenz/saas-fabric` — and the API path is the part after
    /// the host. Accepting both spellings means a manifest and this adapter
    /// cannot disagree about which one is meant.
    pub(crate) fn path<'a>(&self, repository: &'a str) -> &'a str {
        repository
            .strip_prefix(&format!("{}/", self.registry_host))
            .unwrap_or(repository)
    }
}
