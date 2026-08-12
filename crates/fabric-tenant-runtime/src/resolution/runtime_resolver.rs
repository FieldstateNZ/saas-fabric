//! The single place the tenant → DataSource chain is walked.

use std::sync::Arc;

use fabric_connector::ExecutionTarget;
use fabric_core::{LogicalDataSourceName, TenantId};

use crate::resource::LookupError;
use crate::{
    DataSource, DataSourceRegistry, ResolveError, ResolvedDataSource, TenantRegistry, TenantRuntimeBinding,
};

/// Resolves a tenant and a logical data source name into something executable.
///
/// This type exists so the chain has one implementation:
///
/// ```text
/// tenant → binding → logical name → DataSourceId → DataSource → ExecutionTarget
/// ```
///
/// Every step can fail, each failure means something different to an operator,
/// and every one of them rejects the request (§28). Spreading that across
/// callers would mean each caller getting the status codes subtly differently.
///
/// It holds both registries because the chain genuinely crosses them, and
/// because keeping the crossing here is what lets tenant bindings and
/// DataSources be reconciled independently everywhere else.
pub struct RuntimeResolver {
    tenants: Arc<TenantRegistry>,
    data_sources: Arc<DataSourceRegistry>,
}

impl RuntimeResolver {
    /// Builds a resolver over the two registries.
    #[must_use]
    pub const fn new(tenants: Arc<TenantRegistry>, data_sources: Arc<DataSourceRegistry>) -> Self {
        Self {
            tenants,
            data_sources,
        }
    }

    /// The tenant registry, for readiness reporting.
    #[must_use]
    pub const fn tenants(&self) -> &Arc<TenantRegistry> {
        &self.tenants
    }

    /// The DataSource registry, for readiness reporting.
    #[must_use]
    pub const fn data_sources(&self) -> &Arc<DataSourceRegistry> {
        &self.data_sources
    }

    /// Whether both registries have loaded.
    ///
    /// Either being unprimed makes the plane unable to serve, so readiness is
    /// the conjunction.
    #[must_use]
    pub fn is_primed(&self) -> bool {
        self.tenants.is_primed() && self.data_sources.is_primed()
    }

    /// Resolves a tenant's current bindings.
    ///
    /// # Errors
    ///
    /// [`ResolveError::RuntimeUnavailable`] before the first load, or
    /// [`ResolveError::UnknownTenant`] if the tenant is not held.
    pub fn resolve_tenant(&self, tenant: &TenantId) -> Result<Arc<TenantRuntimeBinding>, ResolveError> {
        self.tenants.lookup(tenant).map_err(|error| match error {
            LookupError::Unavailable => ResolveError::RuntimeUnavailable,
            LookupError::NotFound => ResolveError::UnknownTenant(tenant.clone()),
        })
    }

    /// Resolves a logical data source name for a tenant.
    ///
    /// The full chain, and the only supported way to obtain an
    /// [`ExecutionTarget`].
    ///
    /// # Errors
    ///
    /// Any [`ResolveError`]. Each names a different thing to fix.
    pub fn resolve_data_source(
        &self,
        tenant: &TenantId,
        logical: &LogicalDataSourceName,
    ) -> Result<ResolvedDataSource, ResolveError> {
        let binding = self.resolve_tenant(tenant)?;
        let data_binding = binding.data_binding(logical)?;
        let data_source = self.lookup_data_source(logical, &data_binding.data_source)?;

        // The target is assembled from both halves: the DataSource supplies the
        // connector and connection, the tenant binding supplies the isolation.
        // Neither is complete alone, which is the model working as intended.
        let target = ExecutionTarget::new(
            binding.tenant.clone(),
            binding.revision,
            data_source.id.clone(),
            data_source.revision,
            data_source.connector.clone(),
            data_source.connection.clone(),
            data_binding.isolation.clone(),
        );

        Ok(ResolvedDataSource { target, data_source })
    }

    /// Looks up a DataSource, mapping both failures onto the resolution error.
    fn lookup_data_source(
        &self,
        logical: &LogicalDataSourceName,
        id: &fabric_core::DataSourceId,
    ) -> Result<Arc<DataSource>, ResolveError> {
        self.data_sources.lookup(id).map_err(|error| match error {
            LookupError::Unavailable => ResolveError::RuntimeUnavailable,
            // A binding pointing at a DataSource that does not exist is a
            // reconciliation error, never a reason to pick a different one.
            LookupError::NotFound => ResolveError::MissingDataSource {
                logical: logical.clone(),
                data_source: id.clone(),
            },
        })
    }
}
