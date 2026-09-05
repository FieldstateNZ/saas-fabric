//! A configured physical data destination, as the publisher declares it.

use std::collections::BTreeMap;

use fabric_core::{BindingRevision, DataSourceId};

use crate::canonical::to_canonical_bytes;
use crate::{
    ConnectionSelectorDocument, ConnectorId, DataResidencyDocument, DataSourceCapabilitiesDocument,
    PlacementClassDocument, PoolSettingsDocument,
};

/// The publisher's own declaration of a DataSource.
///
/// Mirrors `fabric_tenant_runtime::DataSource` — see
/// [`crate::TenantBindingDocument`] for why this crate declares its own copy.
/// `connection` is deliberately required, matching the consumer: two
/// DataSources that both said nothing about their connection would be two
/// ids and one physical database.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DataSourceDocument {
    /// This DataSource's identity, referenced by tenant bindings.
    pub id: DataSourceId,

    /// The revision of this DataSource's configuration — a resource
    /// revision, independent of any tenant's and of the document's own
    /// [`crate::DocumentRevision`].
    pub revision: BindingRevision,

    /// Which connector executes against this DataSource.
    pub connector: ConnectorId,

    /// How to select the connection within that connector. Required: never
    /// defaulted.
    pub connection: ConnectionSelectorDocument,

    /// The service class this DataSource provides.
    pub placement: PlacementClassDocument,

    /// Where the data physically lives.
    pub residency: DataResidencyDocument,

    /// Pool sizing, applied by reconciliation to the connector.
    #[serde(default)]
    pub pool: PoolSettingsDocument,

    /// What the platform permits this DataSource to be used for. Defaults
    /// closed.
    #[serde(default)]
    pub capabilities: DataSourceCapabilitiesDocument,

    /// Operator-defined labels, emitted with telemetry.
    #[serde(default)]
    pub labels: BTreeMap<String, String>,
}

/// Renders a data-sources document: every DataSource, sorted by id so an
/// unrelated edit produces no diff, then rendered as canonical JSON
/// (two-space indentation, a trailing newline, UTF-8).
///
/// # Errors
///
/// Returns [`serde_json::Error`] only if a value's own `Serialize`
/// implementation fails, which cannot happen for this crate's validated
/// types.
pub fn data_sources_canonical_json(
    data_sources: &[DataSourceDocument],
) -> Result<Vec<u8>, serde_json::Error> {
    let mut sorted = data_sources.to_vec();
    sorted.sort_by(|left, right| left.id.cmp(&right.id));
    to_canonical_bytes(&sorted)
}

#[cfg(test)]
mod tests {
    use fabric_tenant_runtime::DataSource;

    use super::*;

    fn postgres(id: &str) -> DataSourceDocument {
        postgres_connected_by(
            id,
            ConnectionSelectorDocument::Named {
                name: crate::ConnectionName::try_new("acme-prod").unwrap(),
            },
        )
    }

    fn postgres_connected_by(id: &str, connection: ConnectionSelectorDocument) -> DataSourceDocument {
        DataSourceDocument {
            id: DataSourceId::try_new(id).unwrap(),
            revision: BindingRevision::new(4),
            connector: ConnectorId::try_new("postgres-au-east").unwrap(),
            connection,
            placement: PlacementClassDocument::Dedicated,
            residency: DataResidencyDocument {
                region: "au-east".to_owned(),
                jurisdiction: Some("AU".to_owned()),
            },
            pool: PoolSettingsDocument::default(),
            capabilities: DataSourceCapabilitiesDocument {
                writable: true,
                accepts_new_tenants: false,
            },
            labels: BTreeMap::new(),
        }
    }

    #[test]
    fn a_published_data_source_document_deserialises_as_the_runtimes_own_data_source() {
        // Every connection selector variant, not just `Named` -- `Default`
        // and `Secret` are exactly the shapes most likely to drift from the
        // consumer's own shape, since each is struct-shaped for a different
        // reason (see `ConnectionSelectorDocument`'s own rustdoc).
        for connection in [
            ConnectionSelectorDocument::Default {},
            ConnectionSelectorDocument::Named {
                name: crate::ConnectionName::try_new("acme-prod").unwrap(),
            },
            ConnectionSelectorDocument::Secret {
                reference: "tenant/acme/data-primary".to_owned(),
            },
        ] {
            let document = postgres_connected_by("sql-au-east-03", connection.clone());
            let bytes = data_sources_canonical_json(&[document]).unwrap();

            let data_sources: Vec<DataSource> = serde_json::from_slice(&bytes).unwrap();

            assert_eq!(data_sources.len(), 1, "{connection:?}");
            assert_eq!(data_sources[0].id.as_str(), "sql-au-east-03", "{connection:?}");
        }
    }

    #[test]
    fn serialising_the_same_snapshot_twice_produces_identical_bytes() {
        let snapshot = vec![postgres("sql-01"), postgres("sql-02")];

        assert_eq!(
            data_sources_canonical_json(&snapshot).unwrap(),
            data_sources_canonical_json(&snapshot).unwrap()
        );
    }

    #[test]
    fn resources_are_ordered_by_key_so_an_unrelated_edit_produces_no_diff() {
        let forward = vec![postgres("sql-01"), postgres("sql-02")];
        let reversed = vec![postgres("sql-02"), postgres("sql-01")];

        assert_eq!(
            data_sources_canonical_json(&forward).unwrap(),
            data_sources_canonical_json(&reversed).unwrap()
        );
    }
}
