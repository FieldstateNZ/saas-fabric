//! Failures resolving a tenant, and failures loading bindings.

use fabric_core::{DataSourceName, TenantId};

/// Why a tenant's runtime resources could not be resolved.
///
/// Every variant rejects the request. The distinction between them is not
/// cosmetic — it decides the status code, and getting it wrong misleads
/// operators at exactly the wrong moment.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ResolveError {
    /// The registry has no snapshot yet, or has lost it.
    ///
    /// This is a **platform** failure, not a tenant one, and it maps to 503.
    /// It happens during a cold start before the first load completes.
    ///
    /// Reporting this as "unknown tenant" would be actively harmful: every
    /// caller during a restart would be told their tenant had been deleted,
    /// and any client with retry-on-404 suppressed would give up.
    #[error("the tenant runtime is not available")]
    RuntimeUnavailable,

    /// No binding exists for this tenant.
    ///
    /// The tenant may never have existed, or may have been deprovisioned. The
    /// runtime deliberately does not distinguish the two — telling an
    /// unauthenticated-for-this-tenant caller which tenants exist is free
    /// reconnaissance.
    #[error("tenant {0} is not known to the runtime")]
    UnknownTenant(TenantId),

    /// The tenant exists but has no binding for the requested logical data
    /// source.
    ///
    /// A configuration gap: the tenant definition did not declare, say,
    /// `audit`, but a resource in the catalogue asks for it.
    #[error("tenant {tenant} has no binding for data source {data_source}")]
    UnknownDataSource {
        /// The tenant that was resolved.
        tenant: TenantId,
        /// The logical data source that was missing.
        data_source: DataSourceName,
    },
}

/// Why a set of bindings could not be loaded from its source.
///
/// A load failure never clears the registry. The last good snapshot keeps
/// serving, because a momentarily unreadable source is a far better reason to
/// serve slightly stale bindings than to reject every request.
#[derive(Debug, thiserror::Error)]
pub enum BindingSourceError {
    /// The source could not be read.
    ///
    /// The description field is named `origin` rather than `source` because
    /// `thiserror` treats a field called `source` as the error cause, and here
    /// it is only a human-readable label.
    #[error("could not read bindings from {origin}")]
    Unreadable {
        /// A description of the source, for logs.
        origin: String,
        /// The underlying cause.
        #[source]
        cause: Box<dyn std::error::Error + Send + Sync>,
    },

    /// The source was read but its contents could not be understood.
    #[error("bindings from {origin} are malformed: {detail}")]
    Malformed {
        /// A description of the source, for logs.
        origin: String,
        /// What was wrong.
        detail: String,
    },
}
