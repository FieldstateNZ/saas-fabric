//! One declared data source, and the wire document it becomes.

#[cfg(test)]
#[path = "declaration_tests.rs"]
mod declaration_tests;

use std::collections::BTreeMap;

use fabric_core::{BindingRevision, DataSourceId};
use fabric_runtime_publication::{
    ConnectionSelectorDocument, ConnectorId, DataResidencyDocument, DataSourceCapabilitiesDocument,
    DataSourceDocument, FieldName, PlacementClassDocument, PoolSettingsDocument,
};

/// The column every collection on a shared data source carries.
///
/// Not on the wire's own `DataSourceDocument` -- it is a fact about the
/// database, not about one tenant's binding, and ADR 0006 makes it the only
/// isolation a shared source may serve. Placement copies it into each
/// tenant binding when the tenant is placed (ADR 0023 part 2, not built
/// here).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Discriminator {
    /// The column name every collection on this data source carries.
    pub column: FieldName,
}

/// One declared data source: the wire's own shape plus the one fact the
/// wire leaves to the tenant binding.
///
/// # Why this is a separate type from `DataSourceDocument`
///
/// ADR 0023 wants the file an operator hand-edits under break-glass to be
/// the published document in YAML, so a correction to an endpoint cannot
/// disagree with what gets published from it. Reusing the wire's own
/// sub-types (connector, connection, placement, residency, pool,
/// capabilities) is what keeps that true; the discriminator field is the
/// one the wire does not have, because it belongs to the data source
/// itself rather than to any one tenant's binding.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DataSourceDeclaration {
    /// Which data source this is. Referenced by a tenant's placement, and
    /// by nothing else -- an application never sees it.
    pub id: DataSourceId,

    /// The revision the runtime sees. `DataSources::declare` computes this;
    /// nothing a caller sends is trusted.
    pub revision: BindingRevision,

    /// The connector process this data source is configured on.
    pub connector: ConnectorId,

    /// How the connector selects the connection: a name it already holds
    /// configuration for, or a reference to a secret -- never a value.
    pub connection: ConnectionSelectorDocument,

    /// The service class this data source provides.
    pub placement: PlacementClassDocument,

    /// Where the data physically lives.
    pub residency: DataResidencyDocument,

    /// Pool sizing, applied by reconciliation to the connector.
    #[serde(default)]
    pub pool: PoolSettingsDocument,

    /// What the platform permits this data source to be used for.
    #[serde(default)]
    pub capabilities: DataSourceCapabilitiesDocument,

    /// The column every collection on this data source carries. Required
    /// on a shared data source and refused on every other kind (see
    /// validate).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub discriminator: Option<Discriminator>,

    /// Operator-defined labels, carried through to the published document.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub labels: BTreeMap<String, String>,
}

impl DataSourceDeclaration {
    /// The published form.
    ///
    /// Field-by-field, with no logic: the discriminator is the one field
    /// the wire does not carry, and is dropped here. Placement copies it
    /// into a tenant binding when the tenant is placed (ADR 0023 part 2,
    /// not built yet).
    #[must_use]
    pub fn into_document(self) -> DataSourceDocument {
        DataSourceDocument {
            id: self.id,
            revision: self.revision,
            connector: self.connector,
            connection: self.connection,
            placement: self.placement,
            residency: self.residency,
            pool: self.pool,
            capabilities: self.capabilities,
            labels: self.labels,
        }
    }
}
