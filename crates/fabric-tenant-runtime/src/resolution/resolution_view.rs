//! One view of both registries, from which a whole resolution is answered.

use std::sync::Arc;

use fabric_core::{DataSourceId, LogicalDataSourceName, TenantId};

use crate::resource::{LookupError, RegistryResource, ResourceSnapshot};
use crate::{
    CoTenancy, DataSource, DataSourceRegistry, DestinationReuse, ResolveError, TenantRegistry,
    TenantRuntimeBinding,
};

/// A tenant snapshot and a DataSource snapshot, held together for the length
/// of one resolution.
///
/// Every step of the chain -- the binding, the DataSource it names, and the
/// exclusivity facts of both sets -- is read off this one value. Nothing is
/// re-loaded mid-request, so a registry swap that lands while a request is
/// resolving cannot answer one step from one set and the next from another.
/// Which two snapshots it pairs is the resolver's choice: see
/// [`PublishedGeneration`](super::published_generation::PublishedGeneration).
#[derive(Debug, Clone, Default)]
pub(crate) struct ResolutionView {
    tenants: Option<Arc<ResourceSnapshot<TenantRuntimeBinding>>>,
    data_sources: Option<Arc<ResourceSnapshot<DataSource>>>,
}

impl ResolutionView {
    /// What each registry holds now, each loaded once.
    pub(crate) fn of(tenants: &TenantRegistry, data_sources: &DataSourceRegistry) -> Self {
        Self {
            tenants: tenants.current(),
            data_sources: data_sources.current(),
        }
    }

    /// Whether both halves have loaded.
    pub(crate) const fn is_primed(&self) -> bool {
        self.tenants.is_some() && self.data_sources.is_some()
    }

    /// Looks up a tenant binding in this view's tenant set.
    pub(crate) fn tenant(&self, tenant: &TenantId) -> Result<Arc<TenantRuntimeBinding>, ResolveError> {
        lookup(self.tenants.as_deref(), tenant).map_err(|error| match error {
            LookupError::Unavailable => ResolveError::RuntimeUnavailable,
            LookupError::NotFound => ResolveError::UnknownTenant(tenant.clone()),
        })
    }

    /// Looks up the DataSource a binding names in this view's DataSource set.
    pub(crate) fn data_source(
        &self,
        logical: &LogicalDataSourceName,
        id: &DataSourceId,
    ) -> Result<Arc<DataSource>, ResolveError> {
        lookup(self.data_sources.as_deref(), id).map_err(|error| match error {
            LookupError::Unavailable => ResolveError::RuntimeUnavailable,
            // A binding pointing at a DataSource that does not exist is a
            // reconciliation error, never a reason to pick a different one.
            LookupError::NotFound => ResolveError::MissingDataSource {
                logical: logical.clone(),
                data_source: id.clone(),
            },
        })
    }

    /// The tenant set's co-tenancy, `None` while unprimed.
    pub(crate) fn co_tenancy(&self) -> Option<&CoTenancy> {
        self.tenants.as_deref().map(ResourceSnapshot::facts)
    }

    /// The DataSource set's destination reuse, `None` while unprimed.
    pub(crate) fn destination_reuse(&self) -> Option<&DestinationReuse> {
        self.data_sources.as_deref().map(ResourceSnapshot::facts)
    }
}

fn lookup<T: RegistryResource>(
    snapshot: Option<&ResourceSnapshot<T>>,
    key: &T::Key,
) -> Result<Arc<T>, LookupError> {
    let snapshot = snapshot.ok_or(LookupError::Unavailable)?;
    snapshot.get(key).map(Arc::clone).ok_or(LookupError::NotFound)
}
