//! The shape of one NDC connector instance's configuration.
//!
//! The struct and its defaults only. The checks live in
//! [`connector_validation`](super::connector_validation), because what makes a
//! configuration *safe* is a different concern from what it contains — and in
//! this case a considerably more interesting one.

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
    /// Whether any collection is writable.
    #[must_use]
    pub fn has_writes(&self) -> bool {
        self.procedures.values().any(CollectionProcedures::is_writable)
    }
}

#[cfg(test)]
impl NdcConnectorConfig {
    /// A minimal valid configuration, for tests in this module tree.
    pub(super) fn for_test(procedures: BTreeMap<String, CollectionProcedures>) -> Self {
        Self {
            id: ConnectorId::try_new("postgres").unwrap(),
            endpoint: "http://connector:8080".to_owned(),
            timeout_seconds: default_timeout_seconds(),
            connection_name_argument: default_connection_name_argument(),
            connection_string_argument: default_connection_string_argument(),
            procedures,
        }
    }
}
