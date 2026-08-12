//! Configuration for one NDC connector instance.

use std::collections::BTreeMap;

use fabric_connector::ConnectorId;

/// How to reach and use one NDC connector.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NdcConnectorConfig {
    /// The id this connector is registered under, and which tenant bindings
    /// name.
    pub id: ConnectorId,

    /// Base URL of the connector service, without a trailing path.
    pub endpoint: String,

    /// Per-request timeout in seconds.
    #[serde(default = "default_timeout_seconds")]
    pub timeout_seconds: u64,

    /// The request-level argument carrying a named connection.
    ///
    /// `connection_name` matches `ndc-postgres` dynamic connections in `named`
    /// mode. Configurable because the argument name is the connector's to
    /// choose — nothing in the specification fixes it.
    #[serde(default = "default_connection_name_argument")]
    pub connection_name_argument: String,

    /// The request-level argument carrying a full connection string.
    ///
    /// Used only for [`ConnectionSelector::Secret`](fabric_connector::ConnectionSelector),
    /// which is the placement where a tenant has its own database. The value is
    /// a credential and never appears in telemetry.
    #[serde(default = "default_connection_string_argument")]
    pub connection_string_argument: String,

    /// How each collection's writes map onto connector procedures.
    ///
    /// Empty by default, which makes the connector **read-only**. See
    /// [`CollectionProcedures`] for why this cannot be inferred.
    #[serde(default)]
    pub procedures: BTreeMap<String, CollectionProcedures>,
}

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

/// The procedures backing one collection's writes.
///
/// # Why writes need explicit configuration
///
/// Core NDC 0.2 has no generic insert/update/delete. The only mutation
/// operation is invoking a **procedure** the connector declares, and connectors
/// choose their own procedure names and argument shapes — `ndc-postgres`
/// generates `insert_customers`, another connector might expose
/// `customers_create`.
///
/// So this mapping cannot be inferred, and the platform does not try. A
/// collection with no mapping simply cannot be written to, and the attempt is
/// refused. Guessing a procedure name would be unwise for an insert and
/// indefensible for a delete.
#[derive(Debug, Clone, Default, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CollectionProcedures {
    /// The procedure backing inserts.
    #[serde(default)]
    pub insert: Option<ProcedureBinding>,

    /// The procedure backing updates.
    #[serde(default)]
    pub update: Option<ProcedureBinding>,

    /// The procedure backing deletes.
    #[serde(default)]
    pub delete: Option<ProcedureBinding>,
}

impl CollectionProcedures {
    /// Whether any write is possible on this collection.
    #[must_use]
    pub const fn is_writable(&self) -> bool {
        self.insert.is_some() || self.update.is_some() || self.delete.is_some()
    }
}

/// One procedure and the argument names it expects.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProcedureBinding {
    /// The procedure's name in the connector's schema.
    pub procedure: String,

    /// The argument carrying the payload — rows for an insert, field changes
    /// for an update.
    #[serde(default)]
    pub payload_argument: Option<String>,

    /// The argument carrying the predicate, for updates and deletes.
    ///
    /// A mapping for an update or delete that omits this is rejected at
    /// startup: without somewhere to put the predicate, the tenant scoping
    /// added by
    /// [`MutationSpec::for_target`](fabric_connector::MutationSpec::for_target)
    /// would be silently dropped, and the write would reach every tenant's rows.
    #[serde(default)]
    pub filter_argument: Option<String>,
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

        for (collection, procedures) in &self.procedures {
            for (operation, binding) in [("update", &procedures.update), ("delete", &procedures.delete)] {
                if let Some(binding) = binding {
                    if binding.filter_argument.is_none() {
                        return Err(format!(
                            "connector {}: {collection}.{operation} needs a filter_argument, \
                             otherwise the tenant predicate would be dropped and the write would \
                             reach every tenant's rows",
                            self.id
                        ));
                    }
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

    #[test]
    fn a_connector_with_no_procedure_mappings_is_read_only() {
        assert!(!config_with(BTreeMap::new()).has_writes());
    }

    #[test]
    fn a_delete_mapping_without_a_filter_argument_is_rejected_at_startup() {
        // This is the check that matters: with nowhere to put the predicate,
        // the tenant scoping would vanish and the delete would empty the table.
        let config = config_with(BTreeMap::from([(
            "customers".to_owned(),
            CollectionProcedures {
                delete: Some(ProcedureBinding {
                    procedure: "delete_customers".to_owned(),
                    payload_argument: None,
                    filter_argument: None,
                }),
                ..CollectionProcedures::default()
            },
        )]));

        let error = config.validate().unwrap_err();
        assert!(error.contains("filter_argument"));
    }

    #[test]
    fn an_insert_mapping_needs_no_filter_argument() {
        let config = config_with(BTreeMap::from([(
            "customers".to_owned(),
            CollectionProcedures {
                insert: Some(ProcedureBinding {
                    procedure: "insert_customers".to_owned(),
                    payload_argument: Some("objects".to_owned()),
                    filter_argument: None,
                }),
                ..CollectionProcedures::default()
            },
        )]));

        assert!(config.validate().is_ok());
        assert!(config.has_writes());
    }

    #[test]
    fn an_empty_endpoint_is_rejected() {
        let mut config = config_with(BTreeMap::new());
        config.endpoint = "  ".to_owned();

        assert!(config.validate().is_err());
    }
}
