//! Why a tenant's runtime resources could not be resolved.

use fabric_core::{DataSourceId, LogicalDataSourceName, TenantId};

/// A failure somewhere along the tenant → DataSource chain.
///
/// Every variant rejects the request. The distinctions are not cosmetic — they
/// decide the status code, and getting them wrong misleads operators at exactly
/// the wrong moment.
///
/// The last two are both *configuration gaps* rather than caller errors, and
/// they name different things to fix: one means reconciliation did not give the
/// tenant a binding, the other means a binding points at a DataSource that does
/// not exist.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ResolveError {
    /// A registry has no snapshot yet, or has lost it.
    ///
    /// A **platform** failure, not a tenant one. Maps to 503. Happens during a
    /// cold start before the first load completes.
    ///
    /// Reporting this as "unknown tenant" would be actively harmful: every
    /// caller during a restart would be told their tenant had been deleted, and
    /// any client with retry-on-404 suppressed would give up.
    #[error("the tenant runtime is not available")]
    RuntimeUnavailable,

    /// No binding exists for this tenant.
    ///
    /// The tenant may never have existed, or may have been deprovisioned. The
    /// runtime deliberately does not distinguish the two — telling a caller
    /// which tenants exist is free reconnaissance.
    #[error("tenant {0} is not known to the runtime")]
    UnknownTenant(TenantId),

    /// The tenant exists but declared no binding for this logical name.
    ///
    /// A configuration gap: the tenant definition did not declare, say,
    /// `audit`, but a catalogue resource asks for it.
    #[error("tenant {tenant} has no binding for logical data source {logical}")]
    UnboundDataSource {
        /// The tenant that was resolved.
        tenant: TenantId,
        /// The logical name that was missing.
        logical: LogicalDataSourceName,
    },

    /// The binding names a DataSource the runtime does not have.
    ///
    /// Either the DataSource registry has not caught up with a tenant binding
    /// that references a new DataSource, or a DataSource was removed while
    /// tenants were still bound to it. Both are reconciliation errors, and both
    /// fail closed rather than falling back to another DataSource.
    #[error("logical data source {logical} is bound to unknown data source {data_source}")]
    MissingDataSource {
        /// The logical name that was being resolved.
        logical: LogicalDataSourceName,
        /// The DataSource the binding pointed at.
        data_source: DataSourceId,
    },
}
