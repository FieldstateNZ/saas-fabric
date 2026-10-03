//! Everything a registry operation can refuse.
//!
//! Kept in one file because each variant's wording and the adapter errors
//! that become it are one vocabulary, only checkable side by side.
//!
//! Its own enum inside [`ControlPlaneError::Registry`](crate::ControlPlaneError),
//! never mapped to the platform's codes: a registry being registered is not
//! Platform Management failing. Statuses and codes, with the reason for each,
//! are in `errors::status_mapping::registry`.

use fabric_platform_management::RegistryError;

use crate::registries::{Readability, RegistryStoreError};

/// A refused registry operation.
///
/// No variant carries a token, a username, a realm's response or a URL a
/// request supplied: every message is this platform's own words, or the
/// adapter's, which are built the same way.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RegistryFailure {
    /// No registry is registered for this host.
    #[error("no registry is registered for {host}")]
    NotFound {
        /// The host asked for.
        host: String,
    },

    /// The repository is not registered under this registry.
    #[error("{repository} is not registered under this registry")]
    RepositoryNotRegistered {
        /// The repository asked for.
        repository: String,
    },

    /// A registry is already registered for this host.
    #[error("a registry is already registered for {host}")]
    Exists {
        /// The host.
        host: String,
    },

    /// The request breaks one of the rules; the message names which.
    #[error("{0}")]
    Invalid(String),

    /// A registry for the deployment's host must be served where the
    /// deployment reads it.
    #[error(
        "{host} is the deployment's registry; a registration for it must have the deployment's endpoint"
    )]
    EndpointDiffers {
        /// The deployment's host.
        host: String,
    },

    /// The registry's `/v2/` endpoint did not prove.
    #[error("the registry was not proven: {0}")]
    NotProven(String),

    /// The repository's tag listing did not answer through this registry.
    #[error("{repository} is not readable through this registry")]
    RepositoryNotReadable {
        /// The repository.
        repository: String,
    },

    /// The registry's realm refused the credential.
    #[error("{0}")]
    Refused(String),

    /// The registry could not be asked.
    #[error("{0}")]
    Unavailable(String),

    /// The registry's recorded credential was not in the secret partition,
    /// so it has been read anonymously since the last start.
    #[error("the credential for {host} could not be read at the last start; set it again or remove it")]
    CredentialUnreadable {
        /// The registry's host.
        host: String,
    },

    /// The registry records or a credential could not be read or written.
    #[error("the registry store is unavailable")]
    StoreUnavailable,

    /// The stored registry records are not ones this code can read.
    #[error("the stored registry records could not be read")]
    StoreInvalid,
}

impl RegistryFailure {
    /// A failure proving the registry itself.
    pub(crate) fn proving_registry(error: RegistryError) -> Self {
        match error {
            RegistryError::Refused { detail } => Self::NotProven(detail),
            other => Self::asking(other),
        }
    }

    /// A repository's answer, or *not readable through this registry* for
    /// a `401`, `403` or `404` — one message, because Fabric adds no
    /// distinction the registry did not make.
    pub(crate) fn readable<T>(answer: Readability<T>, repository: &str) -> Result<T, Self> {
        match answer {
            Readability::Readable(found) => Ok(found),
            Readability::NotReadable => Err(Self::RepositoryNotReadable {
                repository: repository.to_owned(),
            }),
        }
    }

    /// A failure proving, or listing, a repository through a registry that
    /// is not that answer: a realm that changed, an address refused, a
    /// credential refused, or a registry that could not be asked — each
    /// shown as it was worded.
    pub(crate) fn proving_repository(error: RegistryError) -> Self {
        Self::asking(error)
    }

    /// A refused credential, or a registry that could not be asked.
    fn asking(error: RegistryError) -> Self {
        match error {
            RegistryError::Denied { detail } => Self::Refused(detail),
            RegistryError::Unavailable { detail } => Self::Unavailable(detail),
            RegistryError::Refused { detail } => Self::NotProven(detail),
        }
    }
}

impl From<RegistryStoreError> for RegistryFailure {
    fn from(error: RegistryStoreError) -> Self {
        match error {
            RegistryStoreError::Malformed => Self::StoreInvalid,
            RegistryStoreError::Unavailable | RegistryStoreError::NotPermitted => Self::StoreUnavailable,
        }
    }
}
