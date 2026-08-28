//! Fixture builders shared by this crate's tests.
//!
//! Only compiled under `cfg(test)`. It exists because the resolver, refresher,
//! and source tests all need the same two shapes, and three copies of a
//! `DataSource` literal would drift the moment a field is added.

use std::collections::BTreeMap;

use fabric_connector::{ConnectionName, ConnectionSelector, ConnectorId, FieldName, IsolationModel};
use fabric_core::{BindingRevision, DataSourceId, LogicalDataSourceName, TenantId};

use crate::data_source::{DataResidency, DataSourceCapabilities, PlacementClass, PoolSettings};
use crate::tenant::TenantDataBinding;
use crate::{DataSource, TenantRuntimeBinding};

/// The logical name every fixture binds.
pub(crate) fn primary() -> LogicalDataSourceName {
    LogicalDataSourceName::try_new("primary").unwrap()
}

/// A tenant id.
pub(crate) fn tenant(name: &str) -> TenantId {
    TenantId::try_new(name).unwrap()
}

/// A DataSource id.
pub(crate) fn data_source_id(id: &str) -> DataSourceId {
    DataSourceId::try_new(id).unwrap()
}

/// A connection named after the DataSource that selects it.
///
/// Deliberately derived from the id rather than a constant. Two DataSources
/// sharing one connector *and* one connection name are one physical database
/// wearing two ids, which makes structural isolation across them unenforceable
/// — so a fixture that hardcoded a single connection name quietly built that
/// configuration every time a test registered more than one DataSource. Three
/// tests were doing exactly that before
/// [`DestinationReuse`](crate::DestinationReuse) started noticing.
pub(crate) fn connection_for(id: &str) -> ConnectionSelector {
    ConnectionSelector::Named {
        name: ConnectionName::try_new(id).unwrap(),
    }
}

/// A writable, dedicated DataSource at the given revision.
pub(crate) fn data_source(id: &str, revision: u64) -> DataSource {
    DataSource {
        id: data_source_id(id),
        revision: BindingRevision::new(revision),
        connector: ConnectorId::try_new("postgres").unwrap(),
        connection: connection_for(id),
        placement: PlacementClass::Dedicated,
        residency: DataResidency::in_region("au-east"),
        pool: PoolSettings::default(),
        capabilities: DataSourceCapabilities {
            writable: true,
            accepts_new_tenants: true,
        },
        labels: BTreeMap::new(),
    }
}

/// A DataSource the platform will not accept writes to.
pub(crate) fn read_only_data_source(id: &str) -> DataSource {
    DataSource {
        capabilities: DataSourceCapabilities {
            writable: false,
            accepts_new_tenants: true,
        },
        ..data_source(id, 1)
    }
}

/// A tenant binding whose `primary` points at the given DataSource.
///
/// Uses `Database` isolation, which is structural: it contributes no predicate,
/// so it is only enforceable when the tenant has the DataSource to itself. Two
/// of these on one DataSource is the cross-tenant leak, and resolution refuses
/// it — use [`shared_tenant_binding`] for a fixture that shares.
pub(crate) fn tenant_binding(name: &str, revision: u64, bound_to: &str) -> TenantRuntimeBinding {
    TenantRuntimeBinding::new(tenant(name), BindingRevision::new(revision)).with_data(
        primary(),
        TenantDataBinding::new(data_source_id(bound_to), IsolationModel::Database),
    )
}

/// A tenant binding that is safe to place alongside others on one DataSource.
///
/// Discriminator isolation is the only model that carries its own predicate,
/// and therefore the only one many tenants may share a connection under.
pub(crate) fn shared_tenant_binding(name: &str, revision: u64, bound_to: &str) -> TenantRuntimeBinding {
    TenantRuntimeBinding::new(tenant(name), BindingRevision::new(revision)).with_data(
        primary(),
        TenantDataBinding::new(
            data_source_id(bound_to),
            IsolationModel::Discriminator {
                column: FieldName::try_new("tenant_key").unwrap(),
                value: format!("tenant-{name}"),
            },
        ),
    )
}
