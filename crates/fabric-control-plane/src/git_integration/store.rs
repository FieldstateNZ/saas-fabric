//! Where the integration record is kept between restarts.

use async_trait::async_trait;

use crate::git_integration::GitIntegration;

/// Why the integration record could not be read or written.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum IntegrationStoreError {
    /// The store could not be reached.
    #[error("the integration store is unavailable")]
    Unavailable,

    /// The platform's own credential for the store was refused.
    #[error("the platform's credential for the integration store was refused")]
    NotPermitted,

    /// What is stored is not a record this code can read.
    ///
    /// Reported rather than treated as absence, deliberately. Absence means
    /// "connect one"; a record that will not parse means somebody or something
    /// wrote over it, and offering to overwrite it again would destroy the
    /// evidence of whatever did.
    #[error("the stored integration record could not be read")]
    Malformed,
}

/// Reads and writes one Fabric instance's Git integration record.
///
/// Separate from [`SecretStore`](super::SecretStore) even though both are
/// durable and both are, today, the same backing service. They answer
/// different questions — one holds what the platform may show an operator, the
/// other holds what it must never show anyone — and collapsing them would make
/// "is this safe to return in an API response?" a property of a field name
/// rather than of a type.
#[async_trait]
pub trait IntegrationStore: Send + Sync {
    /// The stored record, or `None` if nothing has been connected.
    ///
    /// # Errors
    ///
    /// Returns [`IntegrationStoreError`] if the store could not be reached or
    /// holds something unreadable. **Absence is not an error.**
    async fn load(&self) -> Result<Option<GitIntegration>, IntegrationStoreError>;

    /// Writes the record, replacing any previous one.
    ///
    /// # Errors
    ///
    /// Returns [`IntegrationStoreError`] if the store could not be reached or
    /// refused the write.
    async fn save(&self, integration: &GitIntegration) -> Result<(), IntegrationStoreError>;

    /// Removes the record. Removing one that is not there is not an error.
    ///
    /// # Errors
    ///
    /// Returns [`IntegrationStoreError`] if the store could not be reached or
    /// refused the removal.
    async fn clear(&self) -> Result<(), IntegrationStoreError>;
}
