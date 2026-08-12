//! Tenants, DataSources, and the catalogue the tests run against.
//!
//! Two tenants on deliberately different placements: `acme` has a dedicated
//! database, `globex` shares a table with a discriminator. One API, two
//! placements, which is the point of §18.

use std::collections::BTreeMap;

use fabric_connector::{ConnectionName, ConnectionSelector, ConnectorId, FieldName, IsolationModel};
use fabric_core::{BindingRevision, DataSourceId, LogicalDataSourceName, TenantId};
use fabric_data_api::ResourceCatalog;
use fabric_tenant_runtime::{
    DataResidency, DataSource, DataSourceCapabilities, PlacementClass, PoolSettings, TenantDataBinding,
    TenantRuntimeBinding,
};

/// A validated field name.
pub fn field(name: &str) -> FieldName {
    FieldName::try_new(name).unwrap()
}

/// A validated tenant id.
pub fn tenant(name: &str) -> TenantId {
    TenantId::try_new(name).unwrap()
}

/// Builds a DataSource with the given connection.
fn data_source(id: &str, revision: u64, connection: &str, placement: PlacementClass) -> DataSource {
    DataSource {
        id: DataSourceId::try_new(id).unwrap(),
        revision: BindingRevision::new(revision),
        connector: ConnectorId::try_new("postgres").unwrap(),
        connection: ConnectionSelector::Named {
            name: ConnectionName::try_new(connection).unwrap(),
        },
        placement,
        residency: DataResidency::in_region("au-east"),
        pool: PoolSettings::default(),
        capabilities: DataSourceCapabilities::default(),
        labels: BTreeMap::new(),
    }
}

/// `acme`'s dedicated database.
pub fn acme_data_source() -> DataSource {
    data_source("acme-prod", 4, "acme-prod", PlacementClass::Dedicated)
}

/// The shared DataSource `globex` sits on.
pub fn discriminator_data_source() -> DataSource {
    data_source("shared-02", 9, "shared-02", PlacementClass::Shared)
}

/// A DataSource the platform will not accept writes to.
pub fn read_only_data_source() -> DataSource {
    DataSource {
        capabilities: DataSourceCapabilities {
            writable: false,
            ..DataSourceCapabilities::default()
        },
        ..data_source("replica-01", 1, "replica-01", PlacementClass::Shared)
    }
}

/// The standard DataSource set.
pub fn data_sources() -> Vec<DataSource> {
    vec![acme_data_source(), discriminator_data_source()]
}

/// The standard tenant set.
pub fn tenants() -> Vec<TenantRuntimeBinding> {
    let primary = LogicalDataSourceName::try_new("primary").unwrap();

    let acme = TenantRuntimeBinding::new(tenant("acme"), BindingRevision::new(7)).with_data(
        primary.clone(),
        TenantDataBinding::new(
            DataSourceId::try_new("acme-prod").unwrap(),
            IsolationModel::Database,
        ),
    );

    let globex = TenantRuntimeBinding::new(tenant("globex"), BindingRevision::new(3)).with_data(
        primary,
        TenantDataBinding::new(
            DataSourceId::try_new("shared-02").unwrap(),
            IsolationModel::Discriminator {
                column: field("tenant_key"),
                value: "tenant-482".to_owned(),
            },
        ),
    );

    vec![acme, globex]
}

/// A tenant bound to a DataSource that is not registered.
pub fn tenant_with_missing_data_source() -> TenantRuntimeBinding {
    TenantRuntimeBinding::new(tenant("orphan"), BindingRevision::new(1)).with_data(
        LogicalDataSourceName::try_new("primary").unwrap(),
        TenantDataBinding::new(
            DataSourceId::try_new("never-deployed").unwrap(),
            IsolationModel::Database,
        ),
    )
}

/// A tenant bound to the read-only DataSource.
pub fn tenant_on_replica() -> TenantRuntimeBinding {
    TenantRuntimeBinding::new(tenant("reader"), BindingRevision::new(1)).with_data(
        LogicalDataSourceName::try_new("primary").unwrap(),
        TenantDataBinding::new(
            DataSourceId::try_new("replica-01").unwrap(),
            IsolationModel::Database,
        ),
    )
}

/// The catalogue: one writable resource, one read-only.
pub fn catalog() -> ResourceCatalog {
    serde_json::from_str(
        r#"{
            "customers": {
                "data_source": "primary",
                "collection": "customers",
                "operations": ["read", "list", "create", "update", "delete"]
            },
            "readOnlyReport": {
                "data_source": "primary",
                "collection": "customers"
            }
        }"#,
    )
    .unwrap()
}
