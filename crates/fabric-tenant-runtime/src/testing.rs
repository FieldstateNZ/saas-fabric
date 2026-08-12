//! Fixture builders shared by this crate's tests.
//!
//! Only compiled under `cfg(test)`. It exists because the resolver, refresher,
//! and source tests all need the same two shapes, and three copies of a
//! `DataSource` literal would drift the moment a field is added.

use std::collections::BTreeMap;

use fabric_connector::{ConnectionName, ConnectionSelector, ConnectorId, IsolationModel};
use fabric_core::{BindingRevision, DataSourceId, DataSourceName, TenantId};

use crate::data_source::{DataResidency, DataSourceCapabilities, PlacementClass, PoolSettings};
use crate::tenant::TenantDataBinding;
use crate::{DataSource, TenantRuntimeBinding};

/// The logical name every fixture binds.
pub(crate) fn primary() -> DataSourceName {
    DataSourceName::try_new("primary").unwrap()
}

/// A tenant id.
pub(crate) fn tenant(name: &str) -> TenantId {
    TenantId::try_new(name).unwrap()
}

/// A DataSource id.
pub(crate) fn data_source_id(id: &str) -> DataSourceId {
    DataSourceId::try_new(id).unwrap()
}

/// A writable, dedicated DataSource at the given revision.
pub(crate) fn data_source(id: &str, revision: u64) -> DataSource {
    DataSource {
        id: data_source_id(id),
        revision: BindingRevision::new(revision),
        connector: ConnectorId::try_new("postgres").unwrap(),
        connection: ConnectionSelector::Named {
            name: ConnectionName::try_new("primary-connection").unwrap(),
        },
        placement: PlacementClass::Dedicated,
        residency: DataResidency::in_region("au-east"),
        pool: PoolSettings::default(),
        capabilities: DataSourceCapabilities::default(),
        labels: BTreeMap::new(),
    }
}

/// A DataSource the platform will not accept writes to.
pub(crate) fn read_only_data_source(id: &str) -> DataSource {
    DataSource {
        capabilities: DataSourceCapabilities {
            writable: false,
            ..DataSourceCapabilities::default()
        },
        ..data_source(id, 1)
    }
}

/// A tenant binding whose `primary` points at the given DataSource.
pub(crate) fn tenant_binding(name: &str, revision: u64, bound_to: &str) -> TenantRuntimeBinding {
    TenantRuntimeBinding::new(tenant(name), BindingRevision::new(revision)).with_data(
        primary(),
        TenantDataBinding::new(data_source_id(bound_to), IsolationModel::Database),
    )
}
