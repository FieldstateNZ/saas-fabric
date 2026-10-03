//! The registry client.

use std::collections::BTreeMap;
use std::sync::Mutex;

mod attached;
mod blob;
mod bounds;
mod build;
mod by_tag;
mod digest;
#[cfg(test)]
mod digest_tests;
mod fetch;
mod http;
mod link;
#[cfg(test)]
mod link_tests;
mod media;
mod provenance;
mod reference;
mod resolve;
mod revisions;
#[cfg(test)]
mod revisions_tests;
mod send;
mod tags;
mod token;
mod verified;
#[cfg(test)]
mod verified_tests;
mod wire;

use fabric_platform_management::{Attached, Registry, RegistryError, Resolved};

/// Reads an OCI registry anonymously, over HTTPS, hashing everything it
/// records.
pub struct OciRegistry {
    /// For manifests, tags, referrers and tokens: follows a redirect only to
    /// the origin the request went to, so a registry can never steer one of
    /// these — or the pull token it carries — somewhere else.
    pub(crate) api: reqwest::Client,

    /// For blobs, which every hosted registry serves from a CDN on another
    /// host: follows no redirect itself. Each hop is followed by hand, to any
    /// origin the transport rule permits, and the pull token is attached only
    /// to a hop on the registry's own origin — never judged against the hop
    /// before, which is how `reqwest` would judge it.
    pub(crate) blobs: reqwest::Client,

    /// The transport rule every blob redirect hop is held to.
    pub(crate) transport: crate::transport::Transport,

    /// Where to talk to it, scheme and all, with no trailing `/`.
    pub(crate) base_url: String,

    /// The same address, parsed: the origin a `Link` must stay on.
    pub(crate) origin: reqwest::Url,

    /// How repositories are *named*, which is not always where they are
    /// served from. A manifest says `ghcr.io/fieldstatenz/saas-fabric`
    /// whatever endpoint this client was pointed at, and a test points it at a
    /// socket without renaming every image in the fixture.
    pub(crate) registry_host: String,

    /// One anonymous pull token per repository.
    ///
    /// A credential, not an answer — and an anonymous one: nothing about
    /// *what was found* is remembered between passes. Held without an
    /// expiry: a token that has aged out comes back as `401`, which is
    /// cheaper to notice than to predict, and the retry path already has to
    /// exist for a token revoked early.
    pub(crate) tokens: Mutex<BTreeMap<String, String>>,

    /// Bytes this client has hashed, by digest: content, never an answer.
    pub(crate) verified: verified::Verified,
}

impl OciRegistry {
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

    async fn resolve(&self, repository: &str, reference: &str) -> Result<Option<Resolved>, RegistryError> {
        self.resolve_reference(repository, reference).await
    }

    async fn component_descriptor(&self, repository: &str, subject: &str) -> Result<Attached, RegistryError> {
        self.attached(repository, subject).await
    }
}
