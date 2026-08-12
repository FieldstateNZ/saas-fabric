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
async fn a_secret_connection_with_no_resolver_is_an_error_not_a_default_connection() {
    // Falling back to the default connection here would run one tenant's
    // query against whatever database the connector happens to point at.
    let selector = ConnectionSelector::Secret {
        reference: SecretRef::new("tenant/acme/data-primary"),
    };

    let error = request_arguments(&config(), &selector, None).await.unwrap_err();

    assert!(matches!(error, ConnectorError::InvalidOperation(_)));
}
