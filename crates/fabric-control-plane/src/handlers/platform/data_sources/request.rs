//! What an operator submits to declare or correct a data source.

#[cfg(test)]
#[path = "request_tests.rs"]
mod request_tests;

use std::collections::BTreeMap;

use fabric_core::{BindingRevision, DataSourceId};
use fabric_platform_management::{
    ConnectionSelectorDocument, ConnectorId, DataResidencyDocument, DataSourceCapabilitiesDocument,
    DataSourceDeclaration, Discriminator, PoolSettingsDocument,
};

use super::placement;
use crate::ControlPlaneError;

/// The body of `PUT /api/platform/data-sources/{dataSourceId}`.
///
/// One entry, **without** `id` and `revision`: the id is the path, and the
/// revision is computed by `DataSources::declare` from what is held, never
/// trusted from the caller. `deny_unknown_fields` refuses either if sent,
/// rather than silently ignoring them — the same rule `Rollback` applies to
/// a digest a browser has just fetched and could send back unchanged.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(crate) struct DataSourceInput {
    connector: ConnectorId,
    connection: ConnectionSelectorDocument,

    /// The console's word for a placement class (`highAvailability`, not
    /// the wire's `high_availability`) — parsed by
    /// [`placement::parse_word`] rather than through `Deserialize`
    /// directly, so an unrecognised value reports every word this platform
    /// accepts instead of a generic "invalid enum variant".
    placement: String,

    residency: DataResidencyDocument,
    pool: PoolInput,
    capabilities: CapabilitiesInput,

    #[serde(default)]
    discriminator: Option<Discriminator>,

    #[serde(default)]
    labels: BTreeMap<String, String>,
}

/// Pool sizing, spelled the console's `camelCase` way on the wire in.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct PoolInput {
    max_connections: u32,
    idle_timeout_seconds: u64,
    acquire_timeout_seconds: u64,
}

/// Capabilities, spelled the console's `camelCase` way on the wire in.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct CapabilitiesInput {
    writable: bool,
    accepts_new_tenants: bool,
}

impl DataSourceInput {
    /// Builds the declaration this input describes, for the id named in the
    /// request path.
    ///
    /// The revision is a placeholder: `DataSources::declare` ignores
    /// whatever a caller sends and computes its own, so any value here is
    /// discarded before it is compared to anything.
    ///
    /// # Errors
    ///
    /// [`ControlPlaneError::InvalidRequest`] if `placement` is not one of
    /// the words this platform accepts.
    pub(crate) fn into_declaration(
        self,
        id: DataSourceId,
    ) -> Result<DataSourceDeclaration, ControlPlaneError> {
        let placement = placement::parse_word(&self.placement)?;

        Ok(DataSourceDeclaration {
            id,
            revision: BindingRevision::new(0),
            connector: self.connector,
            connection: self.connection,
            placement,
            residency: self.residency,
            pool: PoolSettingsDocument {
                max_connections: self.pool.max_connections,
                idle_timeout_seconds: self.pool.idle_timeout_seconds,
                acquire_timeout_seconds: self.pool.acquire_timeout_seconds,
            },
            capabilities: DataSourceCapabilitiesDocument {
                writable: self.capabilities.writable,
                accepts_new_tenants: self.capabilities.accepts_new_tenants,
            },
            discriminator: self.discriminator,
            labels: self.labels,
        })
    }
}
