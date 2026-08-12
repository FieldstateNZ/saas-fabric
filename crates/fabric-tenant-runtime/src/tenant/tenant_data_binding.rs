//! One logical data source, bound for one tenant.

use fabric_connector::IsolationModel;
use fabric_core::DataSourceId;

/// What a tenant's logical data source name resolves to.
///
/// This is the whole of the tenant-specific half of the chain:
///
/// ```text
/// tenant → logical binding (primary) → DataSource → connector → infrastructure
///          └──────── this type ──────┘
/// ```
///
/// Two fields, and both are genuinely per tenant:
///
/// - **`data_source`** — which shared DataSource this tenant's `primary` lives
///   on. The DataSource owns everything physical.
/// - **`isolation`** — how *this* tenant's rows are kept apart from others'
///   within it. A schema name or discriminator value is meaningless outside the
///   context of one tenant, so it cannot live on the shared DataSource.
///
/// Everything else a request needs — connector, connection, pool, region — is
/// read from the DataSource this points at.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TenantDataBinding {
    /// The DataSource this logical name is bound to.
    pub data_source: DataSourceId,

    /// How this tenant's data is isolated within that DataSource (§18).
    pub isolation: IsolationModel,
}

impl TenantDataBinding {
    /// Binds a logical name to a DataSource with the given isolation.
    #[must_use]
    pub const fn new(data_source: DataSourceId, isolation: IsolationModel) -> Self {
        Self {
            data_source,
            isolation,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserialises_a_dedicated_database_binding() {
        let binding: TenantDataBinding =
            serde_json::from_str(r#"{"data_source": "sql-au-east-03", "isolation": {"kind": "database"}}"#)
                .unwrap();

        assert_eq!(binding.data_source.as_str(), "sql-au-east-03");
        assert_eq!(binding.isolation, IsolationModel::Database);
    }

    #[test]
    fn deserialises_a_shared_discriminator_binding() {
        let binding: TenantDataBinding = serde_json::from_str(
            r#"{
                "data_source": "shared-postgres-02",
                "isolation": {"kind": "discriminator", "column": "tenant_key", "value": "tenant-482"}
            }"#,
        )
        .unwrap();

        assert!(binding.isolation.tenant_predicate().is_some());
    }

    #[test]
    fn a_binding_carries_no_physical_configuration() {
        // Connector, endpoint and pool belong to the DataSource. Attempting to
        // set them on a tenant binding must fail rather than be ignored.
        let result = serde_json::from_str::<TenantDataBinding>(
            r#"{"data_source": "sql-au-east-03", "isolation": {"kind": "database"}, "connector": "postgres"}"#,
        );

        assert!(result.is_err());
    }
}
