//! The two ports through which a registry is read: a client for one
//! registry, and the connector that builds clients and installs the set.
//!
//! # Why the control plane builds and installs, and does not read itself
//!
//! Reading a registry — challenges, realms, public addresses, redirects — is
//! the adapter's, and this crate has no transport. What is the control
//! plane's is *which* registries exist, with which credential, for which
//! repositories: so it describes each one as a [`RegistryConnection`], asks
//! the connector the composition root implements for a client, proves
//! through it, and hands the connector the whole set to install where
//! Platform Management reads through it.

use std::any::Any;
use std::collections::BTreeMap;
use std::sync::Arc;

use async_trait::async_trait;
use fabric_component::Repository;
use fabric_platform_management::RegistryError;

use crate::registries::{RegistryConnection, RegistryHost};

/// Whether a repository's tag listing answered through a registry.
///
/// # Why an answer, and not an error
///
/// A `401`, `403` or `404` to a repository's listing is one answer — *not
/// readable through this registry* (ADR 0026 section 5). Every other refusal
/// — a realm that changed, an address the policy refuses — stays an error an
/// operator sees as it was worded, never folded into that answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Readability<T> {
    /// It answered, with this.
    Readable(T),

    /// It answered `401`, `403` or `404`.
    NotReadable,
}

/// One registry, as the control plane asks things of it.
///
/// `Any` so the connector that built a client can recognise its own when it
/// is handed back to install; nothing else downcasts one.
#[async_trait]
pub trait RegistryClient: Any + Send + Sync {
    /// Proves the registry: its `/v2/` endpoint answered through its
    /// challenge, with the credential when one is held. Answers the origin of
    /// the realm the challenge named, if it named one.
    ///
    /// # Errors
    ///
    /// [`RegistryError::Denied`] if the realm refused the credential,
    /// [`RegistryError::Refused`] if the registry did not prove, and
    /// [`RegistryError::Unavailable`] if it could not be asked.
    async fn prove(&self) -> Result<Option<String>, RegistryError>;

    /// Proves a repository is readable through this registry: its tag
    /// listing answered.
    ///
    /// # Errors
    ///
    /// As [`prove`](Self::prove), for everything but the answer.
    async fn prove_repository(&self, repository: &Repository) -> Result<Readability<()>, RegistryError>;

    /// Every tag the repository has published.
    ///
    /// # Errors
    ///
    /// As [`prove_repository`](Self::prove_repository).
    async fn version_tags(&self, repository: &Repository) -> Result<Readability<Vec<String>>, RegistryError>;

    /// Whether this client's realm refused its credential.
    fn credential_refused(&self) -> bool;
}

/// Builds registry clients, and installs the set Platform Management reads
/// through. Implemented by the composition root.
pub trait RegistryConnector: Send + Sync {
    /// A client for one registry.
    ///
    /// # Errors
    ///
    /// A message naming the field that could not be built on, never its
    /// value.
    fn connect(&self, connection: RegistryConnection) -> Result<Arc<dyn RegistryClient>, String>;

    /// Replaces every operator-registered registry Platform Management reads
    /// through with `clients`, all at once. The deployment's own registry
    /// stays, anonymously, for a host `clients` does not name.
    fn install(&self, clients: BTreeMap<RegistryHost, Arc<dyn RegistryClient>>);
}
