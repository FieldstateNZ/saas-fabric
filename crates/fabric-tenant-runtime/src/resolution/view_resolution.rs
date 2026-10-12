//! The tenant → DataSource chain, walked on one view.

use fabric_connector::ExecutionTarget;
use fabric_core::{LogicalDataSourceName, TenantId};

use super::destination_exclusivity;
use super::isolation_enforceability::check_isolation_is_enforceable;
use super::resolution_view::ResolutionView;
use crate::{ResolveError, ResolvedDataSource};

impl ResolutionView {
    /// Resolves a logical data source name for a tenant, every step on this
    /// view.
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
        let binding = self.tenant(tenant)?;
        let data_binding = binding.data_binding(logical)?;
        let data_source = self.data_source(logical, &data_binding.data_source)?;

        // Judged on the same view the binding and DataSource came from, so
        // the exclusivity decision describes the pair being executed.
        let exclusivity = destination_exclusivity::observe(
            self.co_tenancy(),
            self.destination_reuse(),
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
