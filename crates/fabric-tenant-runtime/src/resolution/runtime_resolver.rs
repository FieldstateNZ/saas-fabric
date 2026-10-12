//! The single place the tenant → DataSource chain is walked.

use std::sync::Arc;

use fabric_connector::ExecutionTarget;
use fabric_core::{LogicalDataSourceName, TenantId};

use crate::resolution::destination_exclusivity;
use crate::resolution::isolation_enforceability::check_isolation_is_enforceable;
use crate::resolution::published_generation::PublishedGeneration;
use crate::resolution::resolution_view::ResolutionView;
use crate::{DataSourceRegistry, ResolveError, ResolvedDataSource, TenantRegistry, TenantRuntimeBinding};

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
/// The chain crosses both registries, so every call reads one
/// `ResolutionView` of both and answers each step from it. Built over
/// published state, that view is the last complete generation's
/// (`PublishedGeneration`); built over two independent sources, it is what
/// each registry holds when the call starts.
pub struct RuntimeResolver {
    tenants: Arc<TenantRegistry>,
    data_sources: Arc<DataSourceRegistry>,
    pub(super) published: Option<Arc<PublishedGeneration>>,
}

impl RuntimeResolver {
    /// Builds a resolver over the two registries, each read as it stands.
    #[must_use]
    pub const fn new(tenants: Arc<TenantRegistry>, data_sources: Arc<DataSourceRegistry>) -> Self {
        Self {
            tenants,
            data_sources,
            published: None,
        }
    }

    /// Builds a resolver that reads only the generation `published` holds.
    pub(crate) const fn over_published(
        tenants: Arc<TenantRegistry>,
        data_sources: Arc<DataSourceRegistry>,
        published: Arc<PublishedGeneration>,
    ) -> Self {
        Self {
            tenants,
            data_sources,
            published: Some(published),
        }
    }

    fn view(&self) -> ResolutionView {
        self.published.as_ref().map_or_else(
            || ResolutionView::of(&self.tenants, &self.data_sources, None),
            |published| published.view(),
        )
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
        self.view().is_primed()
    }

    /// Resolves a tenant's current bindings.
    ///
    /// # Errors
    ///
    /// [`ResolveError::RuntimeUnavailable`] before the first load, or
    /// [`ResolveError::UnknownTenant`] if the tenant is not held.
    pub fn resolve_tenant(&self, tenant: &TenantId) -> Result<Arc<TenantRuntimeBinding>, ResolveError> {
        self.view().tenant(tenant)
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
        let view = self.view();
        let binding = view.tenant(tenant)?;
        let data_binding = binding.data_binding(logical)?;
        let data_source = view.data_source(logical, &data_binding.data_source)?;

        // Judged on the same view the binding and DataSource came from, so
        // the exclusivity decision describes the pair being executed.
        let exclusivity = destination_exclusivity::observe(
            view.co_tenancy(),
            view.destination_reuse(),
            &binding.tenant,
            &data_source.id,
        );

        check_isolation_is_enforceable(
            &binding.tenant,
            &data_source,
            &data_binding.isolation,
            exclusivity,
        )?;

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
}
