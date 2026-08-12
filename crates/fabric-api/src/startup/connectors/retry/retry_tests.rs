//! `retry_once` keeps a still-unreachable connector in the pending list and
//! refreshes its recorded reason. Exercised at this level rather than through
//! `spawn` so the test does not depend on wall-clock timing.

use std::sync::Arc;

use fabric_connector::{ConnectorError, DataConnector, SecretResolver};
use fabric_connector_ndc::NdcConnectorConfig;

use super::{retry_once, PendingRetry};
use crate::secrets::EnvSecretResolver;
use crate::startup::connectors::pending_connector::PendingConnector;

/// A connector endpoint nothing is listening on. Port 1 is privileged and
/// conventionally unbound, so the connection is refused immediately — no DNS
/// lookup, no waiting on a real timeout, deterministic in a sandboxed CI
/// environment with no outbound network access.
fn unreachable_connector(id: &str) -> NdcConnectorConfig {
    serde_json::from_str(&format!(
        r#"{{"id":"{id}","endpoint":"http://127.0.0.1:1","http_timeout_seconds":2,"http_connect_timeout_seconds":1}}"#
    ))
    .unwrap()
}

#[tokio::test]
async fn a_connector_that_still_fails_stays_pending_with_a_refreshed_reason() {
    let secrets: Arc<dyn SecretResolver> = Arc::new(EnvSecretResolver);
    let config = unreachable_connector("postgres");
    let placeholder = PendingConnector::new(config.id.clone(), "startup failure".to_owned());

    let still_pending = retry_once(
        vec![PendingRetry::new(config, Arc::clone(&placeholder))],
        &secrets,
    )
    .await;

    assert_eq!(still_pending.len(), 1);

    let Err(ConnectorError::Unreachable { source, .. }) = placeholder.health().await else {
        panic!("a still-failing connector must stay unavailable");
    };

    // The reason came from this retry attempt, not the constructor's.
    assert!(!source.to_string().contains("startup failure"));
}
