//! Registering, proving and removing registries, their credentials and their
//! repositories.
//!
//! Kept in one file because what the service is built from and the state
//! every change shares are one declaration, each field clear only beside
//! the others.

mod access;
mod credential;
mod listed;
mod live;
mod marks;
mod register;
mod registered;
mod remove;
mod repositories;
mod resolve;
mod restore;
mod restored;
mod turn;
mod unregister;
mod versions;
mod withdraw;

use std::collections::BTreeMap;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use fabric_core::Clock;

use crate::git_integration::SecretStore;
use crate::registries::record::SecretId;
use crate::registries::{DeploymentRegistry, RegistryClient, RegistryConnector, RegistryHost, RegistryStore};

pub(crate) use listed::{CredentialState, Listed};
pub(crate) use resolve::Registration;

/// What the registry service is assembled from.
///
/// A struct rather than five positional arguments, two of which are stores
/// a transposition would compile with.
pub struct RegistryServiceParts {
    /// Where the record set is kept.
    pub store: Arc<dyn RegistryStore>,

    /// Where each credential's token is kept.
    pub secrets: Arc<dyn SecretStore>,

    /// Builds clients, and installs the set Platform Management reads
    /// through.
    pub connector: Arc<dyn RegistryConnector>,

    /// Stamps when things were registered, set and proven.
    pub clock: Arc<dyn Clock>,

    /// The deployment's own registry, when Platform Management is
    /// configured.
    pub deployment: Option<DeploymentRegistry>,
}

/// The registries an operator registers (ADR 0026 section 5).
///
/// Exists whether or not Platform Management is configured: registries and
/// the version picker are the catalogue's as much as the platform's.
pub struct RegistryService {
    /// Shared with each change's task, which outlives the request that asked.
    inner: Arc<Inner>,
}

/// What every change and read works on.
pub(super) struct Inner {
    /// Where the record set is kept.
    pub(super) store: Arc<dyn RegistryStore>,

    /// Where each credential's token is kept.
    pub(super) secrets: Arc<dyn SecretStore>,

    /// Builds and installs clients.
    pub(super) connector: Arc<dyn RegistryConnector>,

    /// Stamps records.
    pub(super) clock: Arc<dyn Clock>,

    /// The deployment's own registry.
    pub(super) deployment: Option<DeploymentRegistry>,

    /// One change at a time: see `turn.rs`.
    pub(super) order: tokio::sync::Mutex<()>,

    /// The client each registry is read through now, by host. Never held
    /// across an `await`.
    pub(super) live: std::sync::Mutex<BTreeMap<RegistryHost, Live>>,

    /// Whether each stored credential's realm refused it, by the id that
    /// names it: see `marks.rs`. Never held across an `await`.
    pub(super) marks: std::sync::Mutex<BTreeMap<SecretId, Arc<AtomicBool>>>,
}

/// A registry's client, as it is read through now.
#[derive(Clone)]
pub(super) struct Live {
    /// The client.
    pub(super) client: Arc<dyn RegistryClient>,

    /// Whether its recorded credential could not be read at startup, so it
    /// reads anonymously until the credential is set again.
    pub(super) credential_unreadable: bool,
}

impl RegistryService {
    /// Assembles the service. Nothing is read until [`restore`](Self::restore).
    #[must_use]
    pub fn new(parts: RegistryServiceParts) -> Self {
        Self {
            inner: Arc::new(Inner {
                store: parts.store,
                secrets: parts.secrets,
                connector: parts.connector,
                clock: parts.clock,
                deployment: parts.deployment,
                order: tokio::sync::Mutex::new(()),
                live: std::sync::Mutex::new(BTreeMap::new()),
                marks: std::sync::Mutex::new(BTreeMap::new()),
            }),
        }
    }
}
