//! Connection pool sizing for a published DataSource.

/// The publisher's own declaration of a DataSource's pool configuration.
///
/// Mirrors `fabric_tenant_runtime::PoolSettings`, defaults included, so an
/// operator who omits the block on the wire gets the same numbers the
/// runtime would otherwise apply on its behalf. See
/// [`crate::TenantBindingDocument`] for why this crate declares its own copy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct PoolSettingsDocument {
    /// Maximum concurrent connections the connector may open to this
    /// DataSource, across all tenants bound to it.
    pub max_connections: u32,

    /// How long an idle connection is kept before eviction, in seconds.
    pub idle_timeout_seconds: u64,

    /// How long to wait for a connection before giving up, in seconds.
    pub acquire_timeout_seconds: u64,
}

impl Default for PoolSettingsDocument {
    fn default() -> Self {
        Self {
            max_connections: 20,
            idle_timeout_seconds: 300,
            acquire_timeout_seconds: 5,
        }
    }
}
