//! The fixture every test in this suite starts from: one shared DataSource,
//! two tenants isolated by different values in the same discriminator
//! column, and a one-resource catalogue.
//!
//! ADR 0018 ("The Synthesis Cloud record-isolation seam", item 1) and ADR
//! 0006 together name this the one shape a shared DataSource may serve: a
//! `placement: shared` DataSource may only carry tenants isolated by
//! discriminator, on the same column, each with its own value.

use std::collections::BTreeMap;

use fabric_core::{
    BindingRevision, DataSourceId, LogicalDataSourceName, LogicalResourceName, OperationKind, TenantId,
};
use fabric_runtime_publication::{
    CatalogDocument, CollectionName, ConnectionSelectorDocument, ConnectorId, DataResidencyDocument,
    DataSourceCapabilitiesDocument, DataSourceDocument, DocumentInput, DocumentRevision, FieldName,
    IsolationModelDocument, PlacementClassDocument, PoolSettingsDocument, ResourceDefinitionDocument,
    RuntimeSnapshot, TenantBindingDocument, TenantDataBindingDocument, TenantDataBindings,
};

/// The connector id both the published DataSource and the recording
/// connector answer to.
pub const CONNECTOR_ID: &str = "shared-postgres";

/// The one DataSource the fixture's two tenants share.
pub const DATA_SOURCE_ID: &str = "shared-postgres-01";

/// The discriminator column both tenants are isolated on -- same column,
/// different values, which is exactly what makes this a discriminator
/// fixture rather than two accidentally-identical ones.
pub const DISCRIMINATOR_COLUMN: &str = "tenant_key";

/// acme's value in that column.
pub const ACME_DISCRIMINATOR_VALUE: &str = "tenant-acme-482";

/// globex's value in that column.
pub const GLOBEX_DISCRIMINATOR_VALUE: &str = "tenant-globex-915";

/// The logical name both tenants bind their `primary` data through.
fn logical_primary() -> LogicalDataSourceName {
    LogicalDataSourceName::try_new("primary").unwrap()
}

/// The one shared DataSource's identity.
pub fn data_source_id() -> DataSourceId {
    DataSourceId::try_new(DATA_SOURCE_ID).unwrap()
}

/// The one shared DataSource, at the given resource revision.
pub fn shared_data_source(revision: u64) -> DataSourceDocument {
    DataSourceDocument {
        id: data_source_id(),
        revision: BindingRevision::new(revision),
        connector: ConnectorId::try_new(CONNECTOR_ID).unwrap(),
        connection: ConnectionSelectorDocument::Default {},
        placement: PlacementClassDocument::Shared,
        residency: DataResidencyDocument {
            region: "au-east".to_owned(),
            jurisdiction: None,
        },
        pool: PoolSettingsDocument::default(),
        capabilities: DataSourceCapabilitiesDocument::default(),
        labels: BTreeMap::new(),
    }
}

/// One tenant, discriminator-isolated on the shared DataSource.
pub fn tenant_binding(tenant: &str, discriminator_value: &str, revision: u64) -> TenantBindingDocument {
    let mut data = BTreeMap::new();
    data.insert(
        logical_primary(),
        TenantDataBindingDocument {
            data_source: data_source_id(),
            isolation: IsolationModelDocument::Discriminator {
                column: FieldName::try_new(DISCRIMINATOR_COLUMN).unwrap(),
                value: discriminator_value.to_owned(),
            },
        },
    );

    TenantBindingDocument {
        tenant: TenantId::try_new(tenant).unwrap(),
        revision: BindingRevision::new(revision),
        data: TenantDataBindings::try_new(data).unwrap(),
        configuration: None,
        secrets: None,
        features: BTreeMap::new(),
        storage: BTreeMap::new(),
    }
}

/// The one-resource catalogue: `articles`, exposing only `id` and `title` --
/// never the discriminator column.
pub fn articles_catalog() -> CatalogDocument {
    let mut resources = BTreeMap::new();
    resources.insert(
        LogicalResourceName::try_new("articles").unwrap(),
        ResourceDefinitionDocument {
            data_source: logical_primary(),
            collection: CollectionName::try_new("articles").unwrap(),
            key_field: FieldName::try_new("id").unwrap(),
            operations: vec![OperationKind::Read, OperationKind::List],
            queryable_fields: vec![
                FieldName::try_new("id").unwrap(),
                FieldName::try_new("title").unwrap(),
            ],
        },
    );

    CatalogDocument::new(resources)
}

/// The standard snapshot: both tenants, the one shared DataSource, and the
/// articles catalogue, all at `revision`. Every test in this suite that does
/// not deliberately test a divergence starts from this.
pub fn base_snapshot(revision: u64) -> RuntimeSnapshot {
    RuntimeSnapshot {
        tenants: DocumentInput::new(
            DocumentRevision::new(revision),
            vec![
                tenant_binding("acme", ACME_DISCRIMINATOR_VALUE, revision),
                tenant_binding("globex", GLOBEX_DISCRIMINATOR_VALUE, revision),
            ],
        ),
        data_sources: DocumentInput::new(
            DocumentRevision::new(revision),
            vec![shared_data_source(revision)],
        ),
        catalog: DocumentInput::new(DocumentRevision::new(revision), articles_catalog()),
    }
}
