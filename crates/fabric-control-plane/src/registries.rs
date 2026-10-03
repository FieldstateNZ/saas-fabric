//! Image registries an operator registers (ADR 0026 section 5).
//!
//! # A third integration, and not a third Git one
//!
//! A registry is where a component's images and its descriptor are read
//! from. It is its own product concept with a closed set of kinds — `ghcr`,
//! `dockerHub`, `distribution` — and none of it is the Git integrations':
//! no application, no installation, no callback, and never the App that
//! writes desired state as its credential.
//!
//! # Named by its host, and at most one per host
//!
//! Every image reference already names its registry by host, so that is the
//! key. It is a lookup key into records this platform holds, never a
//! location a request builds: the endpoint a registry is read at was fixed
//! when it was registered, by its kind or by the operator, and changing it is
//! removing the registry and registering another — so a stored credential
//! never moves to a different endpoint.
//!
//! # Proven, then recorded, and written credential first
//!
//! Nothing is recorded that was not proven: a registry's `/v2/` endpoint
//! answered through its challenge, with the credential when one is given, and
//! a repository's tag listing answered. A credential is written before the
//! record that names it, as the Git integrations write a private key before
//! the record — a credential without a record is unreferenced; a record
//! without its credential is a registry that can never present one. Removal
//! runs the other way for the same reason: the live client goes first, then
//! the record, then the credential, whose deletion failing leaves only an
//! unreferenced token, logged.
//!
//! # Where it is kept
//!
//! The record set — hosts, kinds, endpoints, usernames, who set what and when,
//! repositories — is non-secret and may be shown to an operator; it goes
//! through [`RegistryStore`]. A credential's token goes through the existing
//! [`SecretStore`](crate::SecretStore), under a name built only from an id
//! this server minted, never from anything a request carried. That the two
//! are different ports is what makes "is this safe to show?" a property of a
//! type rather than of a field name.
//!
//! # One process's order
//!
//! Every change takes its turn behind one lock and runs in a task of its own,
//! so a request cut off by its timeout still finishes the change it began and
//! two changes never interleave into a record naming one thing and a live
//! client presenting another. The lock is this process's alone and the
//! record set is persisted whole, as every integration's order is; one
//! desired replica does not rule out a second writer while a rolling update
//! overlaps old pod and new. How that overlap is prevented or coordinated,
//! and at what availability cost, is an open gate before registry writes
//! cross an upgrade (ADR 0026 section 5; `service/turn.rs`).

mod connection;
mod connector;
mod deployment;
mod endpoint;
mod errors;
#[cfg(test)]
mod errors_tests;
mod host;
#[cfg(test)]
mod host_tests;
mod in_memory;
mod kind;
mod logging;
mod record;
#[cfg(test)]
mod record_tests;
mod secret_id;
mod service;
#[cfg(test)]
mod service_tests;
mod store;
mod versions;
#[cfg(test)]
mod versions_tests;

pub use connection::{RealmOrigin, RegistryConnection, RegistryCredential};
pub use connector::{Readability, RegistryClient, RegistryConnector};
pub use deployment::DeploymentRegistry;
pub use errors::RegistryFailure;
pub use host::RegistryHost;
pub use in_memory::InMemoryRegistryStore;
pub use kind::RegistryKind;
pub use record::RegistryRecord;
pub use service::{RegistryService, RegistryServiceParts};
pub use store::{RegistryStore, RegistryStoreError};

pub(crate) use record::RegisteredRepository;
pub(crate) use service::{CredentialState, Listed, Registration};
