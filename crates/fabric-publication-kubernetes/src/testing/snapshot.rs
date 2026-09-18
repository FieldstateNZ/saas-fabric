//! One snapshot every adapter test publishes: a tenant on a shared source,
//! and a catalogue with one resource.
#![allow(clippy::unwrap_used)]

use std::collections::BTreeMap;

use fabric_core::{BindingRevision, DataSourceId, LogicalDataSourceName, LogicalResourceName, TenantId};
use fabric_runtime_publication::{
    CatalogDocument, CollectionName, ConnectionName, ConnectionSelectorDocument, ConnectorId,
    DataResidencyDocument, DataSourceCapabilitiesDocument, DataSourceDocument, DocumentInput,
    DocumentRevision, FieldName, IsolationModelDocument, PlacementClassDocument, PoolSettingsDocument,
    ResourceDefinitionDocument, RuntimeSnapshot, TenantBindingDocument, TenantDataBindingDocument,
    TenantDataBindings,
};

pub(crate) fn snapshot(revision: u64) -> RuntimeSnapshot {
    let mut bindings = BTreeMap::new();
    bindings.insert(
        LogicalDataSourceName::try_new("primary").unwrap(),
        TenantDataBindingDocument {
            data_source: DataSourceId::try_new("shared-1").unwrap(),
            isolation: IsolationModelDocument::Discriminator {
                column: FieldName::try_new("tenant_key").unwrap(),
                value: "acme".to_owned(),
            },
        },
    );
    let tenant = TenantBindingDocument {
        tenant: TenantId::try_new("acme").unwrap(),
        revision: BindingRevision::new(1),
        data: TenantDataBindings::try_new(bindings).unwrap(),
        configuration: None,
        secrets: None,
        features: BTreeMap::new(),
        storage: BTreeMap::new(),
    };
    let source = DataSourceDocument {
        id: DataSourceId::try_new("shared-1").unwrap(),
        revision: BindingRevision::new(1),
        connector: ConnectorId::try_new("postgres").unwrap(),
        connection: ConnectionSelectorDocument::Named {
            name: ConnectionName::try_new("shared").unwrap(),
        },
        placement: PlacementClassDocument::Shared,
        residency: DataResidencyDocument {
            region: "nz".to_owned(),
            jurisdiction: None,
        },
        pool: PoolSettingsDocument::default(),
        capabilities: DataSourceCapabilitiesDocument::default(),
        labels: BTreeMap::new(),
    };
    let mut resources = BTreeMap::new();
    resources.insert(
        LogicalResourceName::try_new("customers").unwrap(),
        ResourceDefinitionDocument {
            data_source: LogicalDataSourceName::try_new("primary").unwrap(),
            collection: CollectionName::try_new("customers").unwrap(),
            key_field: FieldName::try_new("id").unwrap(),
            operations: vec![fabric_core::OperationKind::Read],
            queryable_fields: vec![],
        },
    );
    RuntimeSnapshot {
        tenants: DocumentInput::new(DocumentRevision::new(revision), vec![tenant]),
        data_sources: DocumentInput::new(DocumentRevision::new(revision), vec![source]),
        catalog: DocumentInput::new(DocumentRevision::new(revision), CatalogDocument::new(resources)),
    }
}
