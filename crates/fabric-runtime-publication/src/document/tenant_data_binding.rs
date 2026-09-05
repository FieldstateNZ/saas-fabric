//! One logical data source, bound for one tenant, as the publisher declares
//! it.

use fabric_core::DataSourceId;

use crate::IsolationModelDocument;

/// The publisher's own declaration of what a tenant's logical data source
/// name resolves to.
///
/// Mirrors `fabric_tenant_runtime::TenantDataBinding` — see
/// [`crate::TenantBindingDocument`] for why this crate declares its own copy
/// rather than depending on the crate that owns the original.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TenantDataBindingDocument {
    /// The DataSource this logical name is bound to.
    pub data_source: DataSourceId,

    /// How this tenant's data is isolated within that DataSource.
    pub isolation: IsolationModelDocument,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_binding_carries_no_physical_configuration() {
        // Connector, endpoint, and pool belong to the DataSource. Setting
        // them on a tenant binding must fail rather than be ignored.
        let result = serde_json::from_str::<TenantDataBindingDocument>(
            r#"{"data_source": "sql-au-east-03", "isolation": {"kind": "database"}, "connector": "postgres"}"#,
        );

        assert!(result.is_err());
    }
}
