//! How to reach and use one NDC connector.

use std::collections::BTreeMap;

use fabric_connector::ConnectorId;

use crate::config::CollectionProcedures;

/// Ten seconds is long enough for a slow analytical read and short enough that
/// a wedged connector does not hold request slots indefinitely.
const fn default_timeout_seconds() -> u64 {
    10
}

/// Matches `ndc-postgres` named dynamic connections.
fn default_connection_name_argument() -> String {
    "connection_name".to_owned()
}

/// Matches `ndc-postgres` dynamic connection strings.
fn default_connection_string_argument() -> String {
    "connection_string".to_owned()
}

/// One connector instance's configuration.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NdcConnectorConfig {
    /// The id this connector is registered under, and which DataSources name.
    pub id: ConnectorId,

    /// Base URL of the connector service, without a trailing path.
    pub endpoint: String,

    /// Per-request timeout in seconds.
    #[serde(default = "default_timeout_seconds")]
    pub timeout_seconds: u64,

    /// The request-level argument carrying a named connection.
    ///
    /// Configurable because the argument name is the connector's to choose —
    /// nothing in the specification fixes it.
    #[serde(default = "default_connection_name_argument")]
    pub connection_name_argument: String,

    /// The request-level argument carrying a full connection string.
    ///
    /// Used only for [`ConnectionSelector::Secret`](fabric_connector::ConnectionSelector).
    /// The value is a credential and never appears in telemetry.
    #[serde(default = "default_connection_string_argument")]
    pub connection_string_argument: String,

    /// How each collection's writes map onto connector procedures.
    ///
    /// Empty by default, which makes the connector **read-only**.
    #[serde(default)]
    pub procedures: BTreeMap<String, CollectionProcedures>,
}

impl NdcConnectorConfig {
    /// Checks the configuration before the connector is built.
    ///
    /// # Errors
    ///
    /// Returns a message naming the offending setting.
    pub fn validate(&self) -> Result<(), String> {
        if self.endpoint.trim().is_empty() {
            return Err(format!("connector {}: endpoint must not be empty", self.id));
        }

        if self.timeout_seconds == 0 {
            return Err(format!(
                "connector {}: timeout_seconds must be greater than zero",
                self.id
            ));
        }

        self.validate_predicate_arguments()
    }

    /// Requires every update and delete mapping to declare where the predicate
    /// goes.
    ///
    /// This is the check that matters most in this file. Without somewhere to
    /// put the predicate, the tenant scoping would vanish and a delete would
    /// empty the table for every tenant on the DataSource.
    fn validate_predicate_arguments(&self) -> Result<(), String> {
        for (collection, procedures) in &self.procedures {
            for (operation, binding) in procedures.predicate_bearing() {
                let Some(binding) = binding else { continue };

                if binding.filter_argument.is_none() {
                    return Err(format!(
                        "connector {}: {collection}.{operation} needs a filter_argument, otherwise the \
                         tenant predicate would be dropped and the write would reach every tenant's rows",
                        self.id
                    ));
                }
            }
        }

        Ok(())
    }

    /// Whether any collection is writable.
    #[must_use]
    pub fn has_writes(&self) -> bool {
        self.procedures.values().any(CollectionProcedures::is_writable)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ProcedureBinding;

    fn config_with(procedures: BTreeMap<String, CollectionProcedures>) -> NdcConnectorConfig {
        NdcConnectorConfig {
            id: ConnectorId::try_new("postgres").unwrap(),
            endpoint: "http://connector:8080".to_owned(),
            timeout_seconds: 10,
            connection_name_argument: default_connection_name_argument(),
            connection_string_argument: default_connection_string_argument(),
            procedures,
        }
    }

    fn mapping(collection: &str, procedures: CollectionProcedures) -> BTreeMap<String, CollectionProcedures> {
        BTreeMap::from([(collection.to_owned(), procedures)])
    }

    #[test]
    fn a_connector_with_no_procedure_mappings_is_read_only() {
        assert!(!config_with(BTreeMap::new()).has_writes());
    }

    #[test]
    fn a_delete_mapping_without_a_filter_argument_is_rejected_at_startup() {
        let config = config_with(mapping(
            "customers",
            CollectionProcedures {
                delete: Some(ProcedureBinding {
                    procedure: "delete_customers".to_owned(),
                    payload_argument: None,
                    filter_argument: None,
                }),
                ..CollectionProcedures::default()
            },
        ));

        assert!(config.validate().unwrap_err().contains("filter_argument"));
    }

    #[test]
    fn an_insert_mapping_needs_no_filter_argument() {
        let config = config_with(mapping(
            "customers",
            CollectionProcedures {
                insert: Some(ProcedureBinding {
                    procedure: "insert_customers".to_owned(),
                    payload_argument: Some("objects".to_owned()),
                    filter_argument: None,
                }),
                ..CollectionProcedures::default()
            },
        ));

        assert!(config.validate().is_ok());
        assert!(config.has_writes());
    }

    #[test]
    fn an_empty_endpoint_is_rejected() {
        let mut config = config_with(BTreeMap::new());
        config.endpoint = "  ".to_owned();

        assert!(config.validate().is_err());
    }

    #[test]
    fn a_zero_timeout_is_rejected() {
        let mut config = config_with(BTreeMap::new());
        config.timeout_seconds = 0;

        assert!(config.validate().is_err());
    }
}
