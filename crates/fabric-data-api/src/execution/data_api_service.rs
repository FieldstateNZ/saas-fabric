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

    /// The configured limits.
    ///
    /// Note what is deliberately *not* offered alongside it: a way to reach the
    /// catalogue. A `ResourceDefinition` is what field-name validation needs,
    /// and handing one to a handler is what once let request-shape validation
    /// run ahead of authorization. The only `ResourceDefinition` in this crate
    /// now comes out of `prepare`, which authorises first — so the ordering is
    /// a property of what is reachable, not of what each handler remembers to
    /// do. These limits are safe to hand out for the opposite reason: they are
    /// deployment-wide constants that say nothing about any resource.
    #[must_use]
    pub const fn config(&self) -> &DataApiConfig {
        &self.config
    }
}
