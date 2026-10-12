//! Specimen documents that use every field and every variant the publisher
//! can write.
//!
//! The struct patterns below have no `..`: a field added to a document type
//! does not compile until it is added here. Every enum variant comes from
//! one table per enum ([`super::variants`]) that generates both an
//! exhaustive `match` and the specimens, so a new variant does not compile
//! until it is a specimen. A full resource must set
//! every optional field, and a sparse one must leave each out, so the shape
//! also says which fields a reader may find absent. Both are checked when
//! the specimens are built.

use std::collections::BTreeMap;

use super::variants::every_variant;

use fabric_core::{
    BindingRevision, DataSourceId, LogicalDataSourceName, LogicalResourceName, OperationKind, TenantId,
};
use fabric_runtime_publication::{
    CatalogDocument, CollectionName, ConfigurationBindingDocument, ConnectionName,
    ConnectionSelectorDocument, ConnectorId, DataResidencyDocument, DataSourceCapabilitiesDocument,
    DataSourceDocument, FieldName, IsolationModelDocument, PlacementClassDocument, PoolSettingsDocument,
    ResourceDefinitionDocument, SchemaName, StorageBindingDocument, TenantBindingDocument,
    TenantDataBindingDocument, TenantDataBindings,
};

fn every_isolation() -> Vec<IsolationModelDocument> {
    every_variant!(IsolationModelDocument [
        Database {},
        Schema { schema: SchemaName::try_new("tenant_a").unwrap() },
        Discriminator {
            column: FieldName::try_new("tenant_key").unwrap(),
            value: "tenant-a".to_owned(),
        },
    ] unwritten [])
}

fn every_connection() -> Vec<ConnectionSelectorDocument> {
    every_variant!(ConnectionSelectorDocument [
        Default {},
        Named { name: ConnectionName::try_new("primary").unwrap() },
        Secret { reference: "tenant/a/data-primary".to_owned() },
    ] unwritten [])
}

fn every_placement() -> Vec<PlacementClassDocument> {
    every_variant!(PlacementClassDocument [
        Shared,
        Dedicated,
        HighAvailability,
        Regulated,
        Development,
        Ephemeral,
    ] unwritten [])
}

fn every_operation() -> Vec<OperationKind> {
    every_variant!(OperationKind [Read, List, Create, Update, Delete] unwritten [])
}

fn storage(full: bool) -> StorageBindingDocument {
    let binding = StorageBindingDocument {
        provider: "s3".to_owned(),
        container: "tenant-files".to_owned(),
        prefix: full.then(|| "tenant-a/".to_owned()),
        credentials: full.then(|| "tenant/a/storage".to_owned()),
    };
    let StorageBindingDocument {
        provider: _,
        container: _,
        prefix,
        credentials,
    } = &binding;
    assert_eq!([prefix.is_some(), credentials.is_some()], [full; 2]);
    binding
}

fn configuration(full: bool) -> ConfigurationBindingDocument {
    let binding = ConfigurationBindingDocument {
        store: "tenant-config".to_owned(),
        profile: full.then(|| "production".to_owned()),
    };
    let ConfigurationBindingDocument { store: _, profile } = &binding;
    assert_eq!(profile.is_some(), full);
    binding
}

/// A full tenant per isolation kind, every optional field set and every map
/// populated, then one sparse tenant with every optional field left out,
/// and one whose nested documents leave theirs out.
pub fn tenants() -> Vec<TenantBindingDocument> {
    let data_source = DataSourceId::try_new("source-a").unwrap();
    let data = |isolation: IsolationModelDocument| {
        let mut data = BTreeMap::new();
        data.insert(
            LogicalDataSourceName::try_new("primary").unwrap(),
            TenantDataBindingDocument {
                data_source: data_source.clone(),
                isolation,
            },
        );
        TenantDataBindings::try_new(data).unwrap()
    };
    let mut tenants: Vec<TenantBindingDocument> = every_isolation()
        .into_iter()
        .enumerate()
        .map(|(index, isolation)| TenantBindingDocument {
            tenant: TenantId::try_new(format!("full-{index}")).unwrap(),
            revision: BindingRevision::new(1),
            data: data(isolation),
            configuration: Some(configuration(true)),
            secrets: Some("tenant/a".to_owned()),
            features: BTreeMap::from([("beta".to_owned(), true)]),
            storage: BTreeMap::from([("files".to_owned(), storage(true))]),
        })
        .collect();
    tenants.push(TenantBindingDocument {
        tenant: TenantId::try_new("sparse").unwrap(),
        revision: BindingRevision::new(1),
        data: data(IsolationModelDocument::Database {}),
        configuration: None,
        secrets: None,
        features: BTreeMap::new(),
        storage: BTreeMap::new(),
    });
    tenants.push(TenantBindingDocument {
        tenant: TenantId::try_new("sparse-nested").unwrap(),
        revision: BindingRevision::new(1),
        data: data(IsolationModelDocument::Database {}),
        configuration: Some(configuration(false)),
        secrets: Some("tenant/b".to_owned()),
        features: BTreeMap::from([("beta".to_owned(), false)]),
        storage: BTreeMap::from([("files".to_owned(), storage(false))]),
    });
    for tenant in &tenants {
        let TenantBindingDocument {
            tenant: _,
            revision: _,
            data: _,
            configuration,
            secrets,
            features,
            storage,
        } = tenant;
        let full = tenant.tenant.as_str().starts_with("full-");
        let sparse = tenant.tenant.as_str() == "sparse";
        assert!(!full || (configuration.is_some() && secrets.is_some()));
        assert!(!full || (!features.is_empty() && !storage.is_empty()));
        assert!(!sparse || (configuration.is_none() && secrets.is_none()));
    }
    tenants
}

/// One full DataSource per placement class, cycling through every
/// connection kind, then one with every optional field left out.
pub fn data_sources() -> Vec<DataSourceDocument> {
    let connections = every_connection();
    let mut sources: Vec<DataSourceDocument> = every_placement()
        .into_iter()
        .enumerate()
        .map(|(index, placement)| DataSourceDocument {
            id: DataSourceId::try_new(format!("full-{index}")).unwrap(),
            revision: BindingRevision::new(1),
            connector: ConnectorId::try_new("postgres").unwrap(),
            connection: connections[index % connections.len()].clone(),
            placement,
            residency: DataResidencyDocument {
                region: "au-east".to_owned(),
                jurisdiction: Some("AU".to_owned()),
            },
            pool: PoolSettingsDocument {
                max_connections: 10,
                idle_timeout_seconds: 300,
                acquire_timeout_seconds: 5,
            },
            capabilities: DataSourceCapabilitiesDocument {
                writable: true,
                accepts_new_tenants: true,
            },
            labels: BTreeMap::from([("team".to_owned(), "data".to_owned())]),
        })
        .collect();
    assert!(sources.len() >= connections.len());
    sources.push(DataSourceDocument {
        id: DataSourceId::try_new("sparse").unwrap(),
        revision: BindingRevision::new(1),
        connector: ConnectorId::try_new("postgres").unwrap(),
        connection: ConnectionSelectorDocument::Default {},
        placement: PlacementClassDocument::Shared,
        residency: DataResidencyDocument {
            region: "au-east".to_owned(),
            jurisdiction: None,
        },
        pool: PoolSettingsDocument::default(),
        capabilities: DataSourceCapabilitiesDocument::default(),
        labels: BTreeMap::new(),
    });
    for source in &sources {
        let DataSourceDocument {
            id,
            revision: _,
            connector: _,
            connection: _,
            placement: _,
            residency:
                DataResidencyDocument {
                    region: _,
                    jurisdiction,
                },
            pool:
                PoolSettingsDocument {
                    max_connections: _,
                    idle_timeout_seconds: _,
                    acquire_timeout_seconds: _,
                },
            capabilities:
                DataSourceCapabilitiesDocument {
                    writable: _,
                    accepts_new_tenants: _,
                },
            labels,
        } = source;
        let full = id.as_str().starts_with("full-");
        assert_eq!(jurisdiction.is_some(), full);
        assert_eq!(!labels.is_empty(), full);
    }
    sources
}

/// One resource that names every operation and a queryable field.
pub fn catalog() -> CatalogDocument {
    let resource = ResourceDefinitionDocument {
        data_source: LogicalDataSourceName::try_new("primary").unwrap(),
        collection: CollectionName::try_new("articles").unwrap(),
        key_field: FieldName::try_new("id").unwrap(),
        operations: every_operation(),
        queryable_fields: vec![FieldName::try_new("title").unwrap()],
    };
    let ResourceDefinitionDocument {
        data_source: _,
        collection: _,
        key_field: _,
        operations: _,
        queryable_fields,
    } = &resource;
    assert!(!queryable_fields.is_empty());
    CatalogDocument::new(BTreeMap::from([(
        LogicalResourceName::try_new("articles").unwrap(),
        resource,
    )]))
}
