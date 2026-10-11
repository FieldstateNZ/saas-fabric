//! Why a registry offer was refused.

use fabric_core::TenantId;

use super::{RegistryGeneration, RegistryIssuer};

/// Why the issuer registry in a snapshot was refused. Nothing is written.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RegistryRefusal {
    /// One issuer registered twice: which tenant it names would depend on
    /// order, so the registry would not decide the tenant.
    #[error("issuer {issuer} is registered more than once")]
    DuplicateIssuer {
        /// The issuer.
        issuer: RegistryIssuer,
    },

    /// A registration names a tenant the tenants document of the same
    /// generation does not bind: the registry and the tenants document must
    /// come from one tenant list (ADR 0019 G4a).
    #[error(
        "issuer {issuer} names tenant {tenant}, which this publication's tenants document does not bind"
    )]
    TenantUnbound {
        /// The registered issuer.
        issuer: RegistryIssuer,
        /// The tenant no binding names.
        tenant: TenantId,
    },

    /// A registry was published and this snapshot carries none. An edge
    /// holding that generation would be left with nothing to compare to; an
    /// issuer is withdrawn by a generation that leaves it out, never by
    /// dropping the registry.
    #[error("issuer registry generation {held} is published, and this publication carries no registry")]
    Withdrawn {
        /// The generation that is held.
        held: RegistryGeneration,
    },
}
