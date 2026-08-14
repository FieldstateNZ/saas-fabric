//! Tests for routing.

use super::routing::*;
use crate::NdcConnectorConfig;
use async_trait::async_trait;
use fabric_connector::{ConnectionName, ResolvedSecret, SecretRef};
use fabric_connector::{ConnectionSelector, ConnectorError, SecretResolver};
use serde_json::Value;
use std::collections::BTreeMap;
use std::sync::Arc;

struct StubSecrets;

#[async_trait]
impl SecretResolver for StubSecrets {
    async fn resolve(&self, reference: &SecretRef) -> Result<ResolvedSecret, ConnectorError> {
        if reference.as_str() == "missing" {
            return Err(ConnectorError::SecretUnavailable {
                reference: reference.to_string(),
            });
        }

        Ok(ResolvedSecret::new("postgres://user:hunter2@db/acme"))
    }
}

fn config() -> NdcConnectorConfig {
    NdcConnectorConfig::for_test(BTreeMap::new())
}

/// A connector configured for one database and no per-tenant routing.
fn unrouted_config() -> NdcConnectorConfig {
    NdcConnectorConfig {
        connection_name_argument: None,
        connection_string_argument: None,
        ..config()
    }
}

fn resolver() -> Arc<dyn SecretResolver> {
    Arc::new(StubSecrets)
}

#[tokio::test]
async fn a_default_connection_sends_no_routing_arguments() {
    let arguments = request_arguments(&config(), &ConnectionSelector::Default, None)
        .await
        .unwrap();

    assert!(arguments.is_none());
}

#[tokio::test]
async fn a_named_connection_becomes_a_connection_name_argument() {
    let selector = ConnectionSelector::Named {
        name: ConnectionName::try_new("acme-prod").unwrap(),
    };

    let arguments = request_arguments(&config(), &selector, None)
        .await
        .unwrap()
        .unwrap();

    assert_eq!(
        arguments["connection_name"],
        Value::String("acme-prod".to_owned())
    );
}

#[tokio::test]
async fn a_secret_connection_resolves_to_a_connection_string_argument() {
    let selector = ConnectionSelector::Secret {
        reference: SecretRef::new("tenant/acme/data-primary"),
    };

    let arguments = request_arguments(&config(), &selector, Some(&resolver()))
        .await
        .unwrap()
        .unwrap();

    assert_eq!(
        arguments["connection_string"],
        Value::String("postgres://user:hunter2@db/acme".to_owned())
    );
}

#[tokio::test]
async fn an_unresolvable_secret_fails_the_request_rather_than_falling_back() {
    let selector = ConnectionSelector::Secret {
        reference: SecretRef::new("missing"),
    };

    let error = request_arguments(&config(), &selector, Some(&resolver()))
        .await
        .unwrap_err();

    assert!(matches!(error, ConnectorError::SecretUnavailable { .. }));
}

#[tokio::test]
async fn a_named_tenant_on_a_connector_with_no_name_argument_is_refused() {
    // Sending nothing would leave the connector to fall back to whichever
    // connection it was started with -- the cross-tenant read this whole path
    // exists to prevent. This is the one routing failure startup cannot catch:
    // connectors are negotiated before any tenant is resolved.
    let selector = ConnectionSelector::Named {
        name: ConnectionName::try_new("acme-prod").unwrap(),
    };

    let error = request_arguments(&unrouted_config(), &selector, None)
        .await
        .unwrap_err();

    let ConnectorError::InvalidOperation(message) = &error else {
        panic!("expected InvalidOperation, got {error:?}");
    };
    assert!(message.contains("connection_name_argument"), "{message}");
}

#[tokio::test]
async fn a_secret_tenant_on_a_connector_with_no_string_argument_is_refused_before_the_secret_is_read() {
    // Refused ahead of resolution, so a credential is never fetched for a
    // request that could not have used it.
    let selector = ConnectionSelector::Secret {
        reference: SecretRef::new("tenant/acme/data-primary"),
    };

    let error = request_arguments(&unrouted_config(), &selector, Some(&resolver()))
        .await
        .unwrap_err();

    let ConnectorError::InvalidOperation(message) = &error else {
        panic!("expected InvalidOperation, got {error:?}");
    };
    assert!(message.contains("connection_string_argument"), "{message}");
}

#[tokio::test]
async fn a_default_tenant_needs_no_routing_argument_at_all() {
    // The single-database deployment: nothing to select, so nothing to
    // configure and nothing to refuse.
    let arguments = request_arguments(&unrouted_config(), &ConnectionSelector::Default, None)
        .await
        .unwrap();

    assert!(arguments.is_none());
}

#[tokio::test]
async fn the_configured_argument_name_is_what_reaches_the_wire() {
    // Nothing in the specification fixes these names.
    let config = NdcConnectorConfig {
        connection_name_argument: Some("tenant_db".to_owned()),
        ..config()
    };
    let selector = ConnectionSelector::Named {
        name: ConnectionName::try_new("acme-prod").unwrap(),
    };

    let arguments = request_arguments(&config, &selector, None)
        .await
        .unwrap()
        .unwrap();

    assert_eq!(arguments["tenant_db"], Value::String("acme-prod".to_owned()));
    assert!(!arguments.contains_key("connection_name"));
}

#[tokio::test]
async fn a_secret_connection_with_no_resolver_is_an_error_not_a_default_connection() {
    // Falling back to the default connection here would run one tenant's
    // query against whatever database the connector happens to point at.
    let selector = ConnectionSelector::Secret {
        reference: SecretRef::new("tenant/acme/data-primary"),
    };

    let error = request_arguments(&config(), &selector, None).await.unwrap_err();

    assert!(matches!(error, ConnectorError::InvalidOperation(_)));
}
