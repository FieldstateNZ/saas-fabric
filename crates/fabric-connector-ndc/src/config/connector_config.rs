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
///
/// This bounds only the HTTP hop to the connector — see
/// [`NdcConnectorConfig::http_timeout_seconds`] for why the other two clocks
/// in this path (database execution, the overall Data API budget) are
/// deliberately not configured here.
const fn default_http_timeout_seconds() -> u64 {
    10
}

/// Half the total timeout: enough for a TCP handshake and TLS negotiation to a
/// connector on the same network, short enough that a host which never
/// completes a handshake fails fast rather than consuming the whole budget.
const fn default_http_connect_timeout_seconds() -> u64 {
    5
}

/// One connector instance's configuration.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NdcConnectorConfig {
    /// The id this connector is registered under, and which DataSources name.
    pub id: ConnectorId,

    /// Base URL of the connector service, without a trailing path.
    pub endpoint: String,

    /// Total time allowed for one HTTP call to the connector, in seconds.
    ///
    /// # Timeout ownership
    ///
    /// A request passing through this crate is bounded by three separate
    /// clocks, owned by three different places. Naming this field
    /// `http_timeout_seconds` rather than plain `timeout_seconds` is
    /// deliberate, so the scope is legible at every call site:
    ///
    /// 1. **This clock** — the HTTP round trip to the connector process:
    ///    connect, send, wait, and read the response. Owned here, because
    ///    this crate is the thing making the call.
    /// 2. **Database execution inside the connector** — how long
    ///    `ndc-postgres` (or another connector) lets a query run against its
    ///    own database. That is the connector's *own* configuration, not
    ///    settable from this struct at all: `fabric-connector-ndc` has no
    ///    visibility into the connector's internal query planner or pool, and
    ///    inventing a field here that the connector never reads would be
    ///    worse than not having one. Configure it where the connector process
    ///    is deployed.
    /// 3. **The overall Data API request budget** — everything from the
    ///    inbound HTTP request to the outbound response, spanning
    ///    authentication, tenant resolution, and this HTTP call among other
    ///    work. Owned by the host application (`fabric-api`), not this crate:
    ///    a per-connector timeout cannot know about the other work sharing
    ///    that budget.
    ///
    /// This value should be set shorter than (2) would ever need and shorter
    /// than (3) allows for this hop, or the wrong layer ends up deciding when
    /// a slow connector gets cut off.
    #[serde(default = "default_http_timeout_seconds")]
    pub http_timeout_seconds: u64,

    /// Time allowed to establish the connection to the connector, in seconds.
    ///
    /// A subset of [`Self::http_timeout_seconds`], not an addition to it:
    /// reqwest applies this to the connect phase specifically, so a connector
    /// that never completes a TCP/TLS handshake fails fast instead of holding
    /// the request slot for the full HTTP timeout. Must not exceed
    /// `http_timeout_seconds` — checked in `config::validate`.
    #[serde(default = "default_http_connect_timeout_seconds")]
    pub http_connect_timeout_seconds: u64,

    /// The request-level argument carrying a named connection.
    ///
    /// Configurable because the argument name is the connector's to choose —
    /// nothing in the specification fixes it. `ndc-postgres` uses
    /// `connection_name`.
    ///
    /// `None` — the default — says this connector is **not** used for name
    /// routing, and a tenant that asks for it is refused rather than quietly
    /// sent somewhere. Naming an argument is the opposite statement, and one
    /// the platform then holds the connector to at startup: see
    /// `registration::routing_arguments` for why the configuration, rather
    /// than any tenant, is what that check can read.
    #[serde(default)]
    pub connection_name_argument: Option<String>,

    /// The request-level argument carrying a full connection string.
    ///
    /// Used only for [`ConnectionSelector::Secret`](fabric_connector::ConnectionSelector).
    /// The value is a credential and never appears in telemetry.
    ///
    /// Optional on the same terms as [`Self::connection_name_argument`], and
    /// checked the same way.
    #[serde(default)]
    pub connection_string_argument: Option<String>,

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
    /// A minimal valid configuration, for tests anywhere in this crate.
    ///
    /// Names both routing arguments, because most tests in this crate care
    /// about what happens once routing is configured. Tests about the
    /// *unconfigured* case set them back to `None` explicitly, so that case
    /// reads as the deliberate subject of the test rather than a default.
    pub(crate) fn for_test(procedures: BTreeMap<String, CollectionProcedures>) -> Self {
        Self {
            id: ConnectorId::try_new("postgres").unwrap(),
            endpoint: "http://connector:8080".to_owned(),
            http_timeout_seconds: default_http_timeout_seconds(),
            http_connect_timeout_seconds: default_http_connect_timeout_seconds(),
            connection_name_argument: Some("connection_name".to_owned()),
            connection_string_argument: Some("connection_string".to_owned()),
            procedures,
        }
    }
}
