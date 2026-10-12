//! The single place the tenant → DataSource chain is walked.

use std::sync::Arc;

use fabric_core::{LogicalDataSourceName, TenantId};

use crate::publication::DecodedCatalogue;
use crate::resolution::published_generation::PublishedGeneration;
use crate::resolution::ResolutionView;
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
/// [`ResolutionView`] of both and answers each step from it. Built over
/// published state, that view is the last complete generation's, catalogue
/// included (`PublishedGeneration`); built over two independent sources, it
/// is what each registry holds when the call starts, beside the fixed
/// catalogue the composition gave it, if any.
pub struct RuntimeResolver {
    tenants: Arc<TenantRegistry>,
    data_sources: Arc<DataSourceRegistry>,
    catalogue: Option<DecodedCatalogue>,
    pub(super) published: Option<Arc<PublishedGeneration>>,
}

impl RuntimeResolver {
    /// Builds a resolver over the two registries, each read as it stands.
    #[must_use]
    pub const fn new(tenants: Arc<TenantRegistry>, data_sources: Arc<DataSourceRegistry>) -> Self {
        Self {
            tenants,
            data_sources,
            catalogue: None,
            published: None,
        }
    }

    /// Serves `catalogue` in every view of this resolver over independent
    /// sources. A resolver over published state never takes one: its
    /// catalogue is the installed generation's (G8).
    #[must_use]
    pub fn with_catalogue(self, catalogue: DecodedCatalogue) -> Self {
        Self {
            catalogue: Some(catalogue),
            ..self
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
            catalogue: None,
            published: Some(published),
        }
    }

    /// The view every step of one request reads: one atomic load over
    /// published state. A request pins it before it looks the resource up
    /// and keeps it until it executes, so a generation installed meanwhile
    /// reaches the next request, never the rest of this one.
    #[must_use]
    pub fn pin(&self) -> ResolutionView {
        self.published.as_ref().map_or_else(
            || ResolutionView::of(self.catalogue.clone(), &self.tenants, &self.data_sources, None),
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
        self.pin().is_primed()
    }

    /// Resolves a tenant's current bindings.
    ///
    /// # Errors
    ///
    /// [`ResolveError::RuntimeUnavailable`] before the first load, or
    /// [`ResolveError::UnknownTenant`] if the tenant is not held.
    pub fn resolve_tenant(&self, tenant: &TenantId) -> Result<Arc<TenantRuntimeBinding>, ResolveError> {
        self.pin().tenant(tenant)
    }

    /// Resolves a logical data source name for a tenant, on a view pinned
    /// for this call alone. A request that also reads the catalogue pins
    /// one view for both instead: see [`Self::pin`].
    ///
    /// # Errors
    ///
    /// Any [`ResolveError`]. Each names a different thing to fix.
    pub fn resolve_data_source(
        &self,
        tenant: &TenantId,
        logical: &LogicalDataSourceName,
    ) -> Result<ResolvedDataSource, ResolveError> {
        self.pin().resolve_data_source(tenant, logical)
    }
}
