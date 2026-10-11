//! Which tenants a client's lifecycle withholds from publication, without
//! this crate depending on `fabric-client-model`.

use std::collections::BTreeSet;

use fabric_core::TenantId;

/// Answers which tenants must not be published (D01-14).
///
/// A port for the same reason [`crate::RuntimeCatalogueSource`] is one: the
/// lifecycle is recorded in client desired state, which this crate does not
/// read. The control plane implements it over its client documents.
#[async_trait::async_trait]
pub trait TenantLifecycleSource: Send + Sync {
    /// Every tenant whose recorded lifecycle refuses publication: today, a
    /// tombstoned client, or one whose deletion is confirmed.
    ///
    /// # Errors
    ///
    /// [`LifecycleSourceError`] if the lifecycle could not be established.
    /// A pass then publishes nothing rather than guess which tenants are
    /// retired.
    async fn withheld_tenants(&self) -> Result<BTreeSet<TenantId>, LifecycleSourceError>;
}

/// Why [`TenantLifecycleSource::withheld_tenants`] could not answer.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LifecycleSourceError {
    /// A client document could be read but not understood, which needs a
    /// human rather than a retry.
    #[error("{0}")]
    Invalid(String),

    /// Client desired state could not be read at all.
    #[error("{0}")]
    Unavailable(String),
}
