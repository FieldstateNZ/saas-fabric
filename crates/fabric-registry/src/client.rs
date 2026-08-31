//! The registry client.

use std::collections::BTreeMap;
use std::sync::Mutex;

mod resolve;
mod tags;
mod token;
mod wire;

use fabric_platform_management::{Registry, RegistryError, Resolved};

/// Reads an OCI registry anonymously.
pub struct OciRegistry {
    /// The HTTP client, which owns the keep-alive connection pool.
    pub(crate) http: reqwest::Client,

    /// Where to talk to it, scheme and all.
    pub(crate) base_url: String,

    /// How repositories are *named*, which is not always where they are
    /// served from. A manifest says `ghcr.io/fieldstatenz/saas-fabric`
    /// whatever endpoint this client was pointed at, and a test points it at a
    /// socket without renaming every image in the fixture.
    pub(crate) registry_host: String,

    /// One anonymous pull token per repository.
    ///
    /// A credential, not an answer. Nothing about *what was found* is
    /// remembered between passes — a version missing from one repository is a
    /// publishing window, and an adapter that cached that would still believe
    /// it an hour later.
    ///
    /// Held without an expiry. A token that has aged out comes back as `401`,
    /// which is cheaper to notice than to predict, and the retry path already
    /// has to exist for a token revoked early.
    pub(crate) tokens: Mutex<BTreeMap<String, String>>,
}

impl OciRegistry {
    /// Builds a client, e.g. `new("https://ghcr.io", "ghcr.io", 30)`.
    ///
    /// # Errors
    ///
    /// Returns a message if either address is empty, if the base URL is not an
    /// HTTP URL, or if the HTTP client cannot be built. The message names the
    /// field and never its value.
    pub fn new(
        base_url: impl Into<String>,
        registry_host: impl Into<String>,
        timeout_seconds: u64,
    ) -> Result<Self, String> {
        let base_url = base_url.into();
        let registry_host = registry_host.into();

        if registry_host.trim().is_empty() {
            return Err("registry: registry_host is empty".to_owned());
        }

        if !base_url.starts_with("http://") && !base_url.starts_with("https://") {
            return Err("registry: base_url is not an HTTP URL".to_owned());
        }

        if timeout_seconds == 0 {
            // reqwest reads zero as "no timeout", which is the difference
            // between a bounded discovery pass and one that hangs.
            return Err("registry: timeout_seconds is zero".to_owned());
        }

        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(timeout_seconds))
            .build()
            .map_err(|error| format!("registry: could not build an HTTP client: {error}"))?;

        Ok(Self {
            http,
            base_url: base_url.trim_end_matches('/').to_owned(),
            registry_host,
            tokens: Mutex::new(BTreeMap::new()),
        })
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

#[async_trait::async_trait]
impl Registry for OciRegistry {
    async fn tags(&self, repository: &str) -> Result<Vec<String>, RegistryError> {
        self.list_tags(repository).await
    }

    async fn resolve(&self, repository: &str, tag: &str) -> Result<Option<Resolved>, RegistryError> {
        self.resolve_tag(repository, tag).await
    }
}
