//! Problems found when validating reconciled state at load time.

use fabric_core::TenantId;

/// Reconciled state that loaded but does not make sense.
///
/// These are checked when a snapshot is applied rather than when a request
/// arrives, so a bad reconciliation is visible in the logs immediately instead
/// of as a scatter of 500s later.
///
/// Deliberately *not* fatal to the process: one incoherent tenant should not
/// stop the platform serving every other tenant. The offending resource is
/// reported and skipped.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ConfigurationError {
    /// A tenant has no data bindings at all.
    ///
    /// Every data request for it would fail, so it is almost certainly a
    /// mistake in how the tenant was reconciled rather than an intention.
    #[error("tenant {tenant} has no data bindings")]
    TenantHasNoDataBindings {
        /// The tenant in question.
        tenant: TenantId,
    },

    /// A DataSource's own configuration is unusable.
    #[error("{0}")]
    InvalidDataSource(String),
}
