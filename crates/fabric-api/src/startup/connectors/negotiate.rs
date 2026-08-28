//! Negotiating every configured connector, without letting one failure abort
//! the rest.

use std::sync::Arc;

use fabric_connector::{ConnectorRegistry, DataConnector, SecretResolver};
use fabric_connector_ndc::build_ndc_connector;

use crate::config::AppConfig;
use crate::startup::connectors::logging;
use crate::startup::connectors::pending_connector::PendingConnector;
use crate::startup::connectors::retry::PendingRetry;

/// Attempts every configured connector once.
///
/// A connector that fails negotiation is not left unregistered: it is
/// installed under its id as a [`PendingConnector`], so a request naming it
/// fails closed with a clear reason rather than looking exactly like a
/// tenant binding naming a connector nobody configured. It is also handed
/// back for the background retry loop to keep attempting.
///
/// # Errors
///
/// Returns a message when not one connector could be negotiated — see the
/// [`connectors`](super) module docs for why that is the only case this
/// refuses outright.
pub(super) async fn negotiate(
    config: &AppConfig,
    secrets: &Arc<dyn SecretResolver>,
) -> Result<(ConnectorRegistry, Vec<PendingRetry>), String> {
    let mut registry = ConnectorRegistry::new();
    let mut pending = Vec::new();
    let mut negotiated = 0usize;

    for connector in &config.connectors {
        match build_ndc_connector(connector.clone(), Some(Arc::clone(secrets))).await {
            Ok(built) => {
                negotiated += 1;
                registry = registry.with(built as Arc<dyn DataConnector>);
            }
            Err(reason) => {
                logging::negotiation_failed(connector.id.as_str(), &reason);

                let placeholder = PendingConnector::new(connector.id.clone(), reason);
                registry = registry.with(Arc::clone(&placeholder) as Arc<dyn DataConnector>);
                pending.push(PendingRetry::new(connector.clone(), placeholder));
            }
        }
    }

    if negotiated == 0 {
        return Err(format!(
            "none of the {} configured connector(s) could be negotiated; this replica could serve no \
             tenant",
            config.connectors.len()
        ));
    }

    Ok((registry, pending))
}
