//! Where the issuer registry's registrations come from, without this crate
//! depending on `fabric-client-model`.

use fabric_core::TenantId;
use fabric_runtime_publication::{IssuerRegistration, TenantBindingDocument};

use super::exclusion::Exclusion;
use crate::PlacementRecord;

/// Decides which clients' issuers one publication registers (D01-4).
///
/// # Why this is a port
///
/// Whether a client may be routed to is its lifecycle's decision
/// (`fabric_client_model::ClientLifecycle::permits` with the `Route` use),
/// and its issuer is its realm under the deployment's identity origin --
/// both client desired state, which this crate must not depend on, exactly
/// as for [`crate::RuntimeCatalogueSource`]. The control plane implements
/// it; [`crate::RuntimePublisher`] calls it once per pass with the
/// generation it is about to offer, so the registry is decided against the
/// same tenant list it is published with (ADR 0019 G4a).
#[async_trait::async_trait]
pub trait IssuerRegistrySource: Send + Sync {
    /// The registrations for `generation`, and every bound tenant left out.
    ///
    /// # Errors
    ///
    /// [`RegistrySourceError`] if client desired state could not be read.
    async fn registrations(
        &self,
        generation: &RegistryEvidence<'_>,
    ) -> Result<Registrations, RegistrySourceError>;
}

/// The generation a registry is decided for: what is placed, and the
/// tenants document the same publication offers.
pub struct RegistryEvidence<'a> {
    /// Every recorded placement.
    pub placements: &'a [PlacementRecord],
    /// The tenants document this publication offers.
    pub tenants: &'a [TenantBindingDocument],
}

/// What a source decided for one generation.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Registrations {
    /// The issuers registered, each to its tenant.
    pub registered: Vec<IssuerRegistration>,
    /// Bound tenants left out of this generated registry, and why.
    pub excluded: Vec<Excluded>,
}

/// A tenant the tenants document binds that the generated registry leaves
/// out.
///
/// Exclusion from the registry Fabric publishes, and nothing more. No
/// runtime reads the registry, and edge application is platform
/// composition's. Requests for this tenant may still be served by trust
/// configured elsewhere until an applied registry generation shows
/// otherwise. This is not withdrawal or access revocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Excluded {
    /// The tenant.
    pub tenant: TenantId,
    /// Why.
    pub reason: Exclusion,
}

/// Why [`IssuerRegistrySource::registrations`] could not answer.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RegistrySourceError {
    /// Client desired state could not be read. No registry is offered, so
    /// the pass publishes nothing rather than a registry missing a client.
    #[error("{0}")]
    Unavailable(String),
}
