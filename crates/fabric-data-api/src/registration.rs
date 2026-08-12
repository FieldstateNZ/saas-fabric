//! Wiring for the Data API domain.

use std::sync::Arc;

use axum::Router;
use fabric_connector::ConnectorRegistry;
use fabric_identity::IdentityResolver;
use fabric_tenant_runtime::RuntimeResolver;

use crate::{data_routes, logging, DataApiConfig, DataApiService, ResourceCatalog, ResourcePermissions};

/// Validates configuration, builds the service, and returns its router.
///
/// # Errors
///
/// Returns a message if configuration is invalid, the catalogue is empty, or no
/// connectors are registered. All three mean the Data API can serve nothing,
/// which is almost certainly a mistake in how it was configured rather than an
/// intention — and finding out at startup beats finding out per request.
pub fn build_data_api(
    config: &DataApiConfig,
    catalog: ResourceCatalog,
    permissions: ResourcePermissions,
    runtime: Arc<RuntimeResolver>,
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
        runtime,
        connectors,
        catalog,
        permissions,
        config.clone(),
    ));

    Ok(data_routes(crate::DataApiState { service, identity }))
}
