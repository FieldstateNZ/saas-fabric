//! Negotiating the configured connectors.

use std::sync::Arc;

use fabric_connector::{ConnectorRegistry, DataConnector, SecretResolver};
use fabric_connector_ndc::build_ndc_connector;

use crate::config::AppConfig;

/// Negotiates every configured connector and registers it.
///
/// Each connector performs its `GET /capabilities` and `GET /schema` here, once
/// — the same principle as §6 applied to connectors: discovery belongs before
/// request handling, not inside it.
///
/// # Errors
///
/// Returns a message if any connector is unreachable, speaks an incompatible
/// specification version, or has invalid configuration. Fatal by design: a
/// connector that cannot be negotiated cannot serve the tenants bound to it,
/// and finding out at boot beats finding out under load.
pub(super) async fn build(
    config: &AppConfig,
    secrets: &Arc<dyn SecretResolver>,
) -> Result<ConnectorRegistry, String> {
    let mut registry = ConnectorRegistry::new();

    for connector in &config.connectors {
        let built = build_ndc_connector(connector.clone(), Some(Arc::clone(secrets))).await?;
        registry = registry.with(built as Arc<dyn DataConnector>);
    }

    Ok(registry)
}
