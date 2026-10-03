//! Where the registry record set is kept between restarts.

use async_trait::async_trait;

use crate::registries::RegistryRecord;

/// Why the registry record set could not be read or written.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RegistryStoreError {
    /// The store could not be reached.
    #[error("the registry store is unavailable")]
    Unavailable,

    /// The platform's own credential for the store was refused.
    #[error("the platform's credential for the registry store was refused")]
    NotPermitted,

    /// What is stored is not a record set this code can read.
    ///
    /// Reported rather than treated as absence, deliberately, as the Git
    /// integration's record is: absence means "register one", and a record
    /// set that will not parse means somebody or something wrote over it.
    /// Saving over it as though it were empty would destroy every registry in
    /// it along with the evidence.
    #[error("the stored registry records could not be read")]
    Malformed,
}

/// Reads and writes one Fabric instance's registry records, as one set.
///
/// # Why the whole set, and not a record per registry
///
/// There are a handful, every change is made in this process's order, and
/// one entry needs no listing — the instance partition offers none — and
/// no second place for two halves of a change to disagree.
///
/// Separate from [`SecretStore`](crate::SecretStore) for the reason the Git
/// integration's two ports are: this holds what an operator may be shown,
/// that holds what nobody may.
#[async_trait]
pub trait RegistryStore: Send + Sync {
    /// Every registry recorded. **Nothing recorded is an empty set, not an
    /// error.**
    ///
    /// # Errors
    ///
    /// [`RegistryStoreError`] if the store could not be reached, or holds
    /// something unreadable.
    async fn load(&self) -> Result<Vec<RegistryRecord>, RegistryStoreError>;

    /// Replaces the whole set.
    ///
    /// # Errors
    ///
    /// [`RegistryStoreError`] if the store could not be reached or refused
    /// the write.
    async fn save(&self, records: &[RegistryRecord]) -> Result<(), RegistryStoreError>;
}
