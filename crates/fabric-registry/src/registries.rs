//! Reading each repository through the registry its name belongs to.
//!
//! # Why an unknown host is refused, never sent to a default
//!
//! A repository's host names a registry; the host is a lookup key into the
//! registries held here, never a location a request is built from (ADR 0026
//! section 5). A host with no registry is refused, naming the host: falling
//! through to a default would read `quay.io/x` from GHCR, or present one
//! registry's credential to a repository of another.
//!
//! # Why the whole map is replaced at once
//!
//! Registering, replacing or removing a registry builds a new map and swaps
//! it in, so a pass in flight reads through the map it started with and a
//! replaced credential's client — with every token it obtained — is simply
//! dropped when the last reader is done with it.

use std::collections::BTreeMap;
use std::sync::Arc;

use arc_swap::ArcSwap;
use fabric_platform_management::{Attached, Registry, RegistryError, Resolved};

use crate::client::OciRegistry;

/// Registries by the host their repositories are named under.
pub struct Registries {
    /// `docker.io`, `ghcr.io`, `host:port` to the client for it.
    by_host: ArcSwap<BTreeMap<String, Arc<OciRegistry>>>,
}

impl Registries {
    /// Routes to `by_host`, keyed by each registry's naming host.
    #[must_use]
    pub fn new(by_host: BTreeMap<String, Arc<OciRegistry>>) -> Self {
        Self {
            by_host: ArcSwap::from_pointee(by_host),
        }
    }

    /// Replaces every registry at once.
    pub fn replace(&self, by_host: BTreeMap<String, Arc<OciRegistry>>) {
        self.by_host.store(Arc::new(by_host));
    }

    /// The registry for `host`, for the control plane's proofs and version
    /// listings.
    #[must_use]
    pub fn get(&self, host: &str) -> Option<Arc<OciRegistry>> {
        self.by_host.load().get(host).cloned()
    }

    /// The registry `repository` is named under.
    ///
    /// # Errors
    ///
    /// [`RegistryError::Refused`] naming the host when no registry is held
    /// for it, or when the repository names no host at all.
    fn route(&self, repository: &str) -> Result<Arc<OciRegistry>, RegistryError> {
        let Some((host, _)) = repository.split_once('/') else {
            return Err(RegistryError::Refused {
                detail: "a repository names no registry host".to_owned(),
            });
        };
        self.get(host).ok_or_else(|| RegistryError::Refused {
            detail: format!("no registry is registered for {host}"),
        })
    }
}

#[async_trait::async_trait]
impl Registry for Registries {
    async fn tags(&self, repository: &str) -> Result<Vec<String>, RegistryError> {
        self.route(repository)?.tags(repository).await
    }

    async fn resolve(&self, repository: &str, reference: &str) -> Result<Option<Resolved>, RegistryError> {
        self.route(repository)?.resolve(repository, reference).await
    }

    async fn component_descriptor(&self, repository: &str, subject: &str) -> Result<Attached, RegistryError> {
        self.route(repository)?
            .component_descriptor(repository, subject)
            .await
    }
}
