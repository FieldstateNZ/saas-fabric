//! What the platform permits a published DataSource to be used for.

/// The publisher's own declaration of a DataSource's capabilities.
///
/// Mirrors `fabric_tenant_runtime::DataSourceCapabilities` — see
/// [`crate::TenantBindingDocument`] for why this crate declares its own copy.
/// Both switches default to `false`: a DataSource must *declare* what it
/// permits rather than defaulting to permissive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct DataSourceCapabilitiesDocument {
    /// Whether the platform permits write operations against this
    /// DataSource.
    pub writable: bool,

    /// Whether reconciliation may bind new tenants to this DataSource.
    pub accepts_new_tenants: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_undeclared_data_source_permits_nothing() {
        let capabilities: DataSourceCapabilitiesDocument = serde_json::from_str("{}").unwrap();

        assert!(!capabilities.writable);
        assert!(!capabilities.accepts_new_tenants);
    }
}
