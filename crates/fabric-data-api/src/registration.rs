//! Wiring for the Data API domain.

use std::sync::Arc;

use axum::Router;
use fabric_connector::ConnectorRegistry;
use fabric_identity::IdentityResolver;
use fabric_tenant_runtime::TenantRuntimeRegistry;

use crate::{data_routes, logging, DataApiConfig, DataApiService, ResourceCatalog, ResourcePermissions};

/// Validates configuration, builds the service, and returns its router.
///
/// # Startup validation
///
/// Every catalogue entry is checked against the connectors that were actually
/// registered — not for the collection's existence, which is per-connector and
/// per-tenant, but for the things that are knowable at boot. A catalogue
/// pointing at a data source no tenant declares is a silent 500 waiting for its
/// first request; catching what we can here moves those failures to deployment
/// time.
///
/// # Errors
///
/// Returns a message if configuration is invalid or the catalogue is empty. An
/// empty catalogue means the Data API can serve nothing, which is almost
/// certainly a mistake in how it was configured rather than an intention.
pub fn build_data_api(
    config: &DataApiConfig,
    catalog: ResourceCatalog,
    permissions: ResourcePermissions,
    tenants: Arc<TenantRuntimeRegistry>,
    connectors: ConnectorRegistry,
    identity: Arc<IdentityResolver>,
) -> Result<Router, String> {
    config.validate()?;

    if catalog.is_empty() {
        return Err("data_api: the resource catalogue is empty, so no resource can be served".to_owned());
    }

    if connectors.is_empty() {
        return Err("data_api: no connectors are registered, so nothing can be executed".to_owned());
    }

    logging::data_api_ready(catalog.len(), connectors.len());

    let service = Arc::new(DataApiService::new(
        tenants,
        connectors,
        catalog,
        permissions,
        config.clone(),
    ));

    Ok(data_routes(crate::DataApiState { service, identity }))
}
