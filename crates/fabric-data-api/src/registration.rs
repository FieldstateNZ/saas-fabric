//! Wiring for the Data API domain.

use std::sync::Arc;

use axum::Router;
use fabric_connector::ConnectorRegistry;
use fabric_identity::IdentityResolver;
use fabric_tenant_runtime::RuntimeResolver;

use crate::{data_routes, logging, DataApiConfig, DataApiService, DataAuthorization};

/// Validates configuration, builds the service, and returns its router.
///
/// # Errors
///
/// Returns a message if configuration is invalid or no connectors are
/// registered. Either means the Data API can serve nothing, which is almost
/// certainly a mistake in how it was configured rather than an intention --
/// and finding out at startup beats finding out per request.
///
/// The catalogue is not an argument: it is part of the view `runtime`
/// installs (G8). A catalogue the host cannot serve, an empty one included,
/// is refused by its decoder ([`decode_catalogue`](crate::decode_catalogue))
/// before any view holds it.
pub fn build_data_api(
    config: &DataApiConfig,
    authorization: DataAuthorization,
    runtime: Arc<RuntimeResolver>,
    connectors: ConnectorRegistry,
    identity: Arc<IdentityResolver>,
) -> Result<Router, String> {
    config.validate()?;

    if connectors.is_empty() {
        return Err("data_api: no connectors are registered, so nothing can be executed".to_owned());
    }

    logging::data_api_ready(connectors.len());

    let service = Arc::new(DataApiService::new(
        runtime,
        connectors,
        authorization,
        config.clone(),
    ));

    Ok(data_routes(crate::DataApiState { service, identity }))
}
