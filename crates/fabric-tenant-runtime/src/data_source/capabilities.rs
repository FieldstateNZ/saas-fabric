//! What a DataSource permits, independently of what its connector can do.

/// Platform-level constraints on a DataSource.
///
/// # Distinct from connector capabilities
///
/// [`ConnectorCapabilities`](fabric_connector::ConnectorCapabilities) describe
/// what a backend *can express* — which predicates it supports, whether it
/// accepts writes at all. These describe what this DataSource is *permitted to
/// be used for*, which is a platform decision rather than a technical one.
///
/// A read replica is the clearest case: the connector can express writes
/// perfectly well, and the replica will reject them at some depth with a
/// vendor-specific error. Declaring `writable: false` here means the platform
/// refuses the write before it leaves the process, with a clear message and no
/// wasted round trip.
///
/// Both are checked. Either saying no is a no — the checks compose in the
/// fail-closed direction (§28).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields, default)]
pub struct DataSourceCapabilities {
    /// Whether this DataSource accepts writes.
    ///
    /// Defaults to `true`, because a DataSource that cannot be written to is
    /// the unusual case and should be declared deliberately.
    pub writable: bool,

    /// Whether tenants may be newly placed here.
    ///
    /// Set to `false` to drain a DataSource: existing tenants keep working,
    /// reconciliation stops adding more. The runtime reports it; placement is
    /// reconciliation's decision to make.
    pub accepts_new_tenants: bool,
}

impl Default for DataSourceCapabilities {
    fn default() -> Self {
        Self {
            writable: true,
            accepts_new_tenants: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_data_source_is_writable_unless_declared_otherwise() {
        assert!(DataSourceCapabilities::default().writable);
    }

    #[test]
    fn a_read_replica_can_be_declared_read_only() {
        let capabilities: DataSourceCapabilities = serde_json::from_str(r#"{"writable": false}"#).unwrap();

        assert!(!capabilities.writable);
        // Draining and read-only are independent switches.
        assert!(capabilities.accepts_new_tenants);
    }
}
