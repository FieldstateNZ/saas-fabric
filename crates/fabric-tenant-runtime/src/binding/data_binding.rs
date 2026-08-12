//! Where one of a tenant's logical data sources currently lives.

use fabric_connector::{ConnectionSelector, ConnectorId, IsolationModel};

/// The physical resolution of one logical data source for one tenant.
///
/// This is the payload behind an entry like `data.primary → sql-au-east-03/acme-prod`
/// from §7. Note what it is *not*: it holds no connection string, no password,
/// and no host name. It holds a connector to route to, a way to select a
/// connection within that connector, and the isolation model in force.
///
/// Applications must never see this type (§7). It is reachable only through
/// [`TenantRuntimeBinding::execution_target`](crate::TenantRuntimeBinding::execution_target),
/// which produces the connector-facing
/// [`ExecutionTarget`](fabric_connector::ExecutionTarget).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DataBinding {
    /// Which connector executes operations for this data source.
    pub connector: ConnectorId,

    /// Which connection within that connector.
    #[serde(default = "default_connection")]
    pub connection: ConnectionSelector,

    /// How this tenant's rows are kept apart from other tenants'.
    pub isolation: IsolationModel,
}

/// Connectors that serve a single database need no explicit selection.
fn default_connection() -> ConnectionSelector {
    ConnectionSelector::Default
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserialises_a_dedicated_database_binding() {
        let binding: DataBinding = serde_json::from_str(
            r#"{
                "connector": "postgres-au-east",
                "connection": {"kind": "named", "name": "acme-prod"},
                "isolation": {"kind": "database"}
            }"#,
        )
        .unwrap();

        assert_eq!(binding.connector.as_str(), "postgres-au-east");
        assert_eq!(binding.isolation, IsolationModel::Database);
    }

    #[test]
    fn deserialises_a_shared_discriminator_binding() {
        let binding: DataBinding = serde_json::from_str(
            r#"{
                "connector": "postgres-shared",
                "connection": {"kind": "named", "name": "shared-02"},
                "isolation": {
                    "kind": "discriminator",
                    "column": "tenant_key",
                    "value": "tenant-482"
                }
            }"#,
        )
        .unwrap();

        assert!(binding.isolation.tenant_predicate().is_some());
    }

    #[test]
    fn the_connection_defaults_when_a_connector_serves_one_database() {
        let binding: DataBinding =
            serde_json::from_str(r#"{"connector": "postgres", "isolation": {"kind": "database"}}"#).unwrap();

        assert_eq!(binding.connection, ConnectionSelector::Default);
    }

    #[test]
    fn an_unexpected_field_is_rejected_rather_than_ignored() {
        // A typo in reconciliation output must not be silently dropped — a
        // misspelled "isolaton" would otherwise leave a tenant on the default.
        let result = serde_json::from_str::<DataBinding>(
            r#"{"connector": "postgres", "isolation": {"kind": "database"}, "typo": 1}"#,
        );

        assert!(result.is_err());
    }
}
