//! The service that executes Data API operations.

use std::sync::Arc;

use fabric_connector::ConnectorRegistry;
use fabric_tenant_runtime::RuntimeResolver;

use crate::{DataApiConfig, ResourceCatalog, ResourcePermissions};

/// Executes Data API operations.
///
/// This is where the platform's core promise is kept:
///
/// ```text
/// bearer token → tenant_id → tenant binding → DataSource → connector
/// ```
///
/// The middle of that chain belongs to
/// [`RuntimeResolver`](fabric_tenant_runtime::RuntimeResolver), which this
/// holds rather than reaching into registries itself. That keeps the tenant →
/// DataSource walk in one place with one set of error mappings, and leaves this
/// crate responsible for the two ends: what a logical resource means, and what
/// a caller is allowed to do with it.
///
/// Nothing here takes a tenant as a parameter. The only source is the
/// [`TenantIdentity`](fabric_identity::TenantIdentity), which came from the
/// bearer token (§10, §11).
pub struct DataApiService {
    pub(super) runtime: Arc<RuntimeResolver>,
    pub(super) connectors: ConnectorRegistry,
    pub(super) catalog: ResourceCatalog,
    pub(super) permissions: ResourcePermissions,
    pub(super) config: DataApiConfig,
}

impl DataApiService {
    /// Builds the service. Called from [`build_data_api`](crate::build_data_api).
    #[must_use]
    pub const fn new(
        runtime: Arc<RuntimeResolver>,
        connectors: ConnectorRegistry,
        catalog: ResourceCatalog,
        permissions: ResourcePermissions,
        config: DataApiConfig,
    ) -> Self {
        Self {
            runtime,
            connectors,
            catalog,
            permissions,
            config,
        }
    }

    /// The catalogue, for handlers that need a resource definition before
    /// dispatching.
    #[must_use]
    pub const fn catalog(&self) -> &ResourceCatalog {
        &self.catalog
    }

    /// The configured limits.
    #[must_use]
    pub const fn config(&self) -> &DataApiConfig {
        &self.config
    }
}
