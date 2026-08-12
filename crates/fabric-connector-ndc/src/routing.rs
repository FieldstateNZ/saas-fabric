//! Turning a tenant's connection selector into NDC request arguments.

use std::collections::BTreeMap;
use std::sync::Arc;

use fabric_connector::{ConnectionSelector, ConnectorError, SecretResolver};
use serde_json::Value;

use crate::NdcConnectorConfig;

/// Builds the `request_arguments` map that routes a request to the tenant's
/// physical connection.
///
/// This is the single point where per-tenant placement meets the protocol.
/// Everything upstream has been resolving *which* database; this turns that
/// answer into something the connector will act on.
///
/// # Errors
///
/// [`ConnectorError::SecretUnavailable`] if a credential cannot be resolved, or
/// [`ConnectorError::InvalidOperation`] if a secret-backed connection is
/// configured without a resolver. Both fail the request closed — there is no
/// fallback to a default connection, because a fallback would silently execute
/// one tenant's query against another tenant's database (§28).
pub(crate) async fn request_arguments(
    config: &NdcConnectorConfig,
    selector: &ConnectionSelector,
    secrets: Option<&Arc<dyn SecretResolver>>,
) -> Result<Option<BTreeMap<String, Value>>, ConnectorError> {
    match selector {
        // The connector serves one database; there is nothing to select.
        ConnectionSelector::Default => Ok(None),

        ConnectionSelector::Named { name } => Ok(Some(BTreeMap::from([(
            config.connection_name_argument.clone(),
            Value::String(name.to_string()),
        )]))),

        ConnectionSelector::Secret { reference } => {
            let resolver = secrets.ok_or_else(|| {
                ConnectorError::InvalidOperation(format!(
                    "connector {} has a tenant bound to secret {reference} but no secret resolver \
                     was configured",
                    config.id
                ))
            })?;

            let secret = resolver.resolve(reference).await?;

            // `expose` is called here and only here for connection strings. The
            // value goes straight into the request body and is never logged,
            // never put in a span field, and never included in an error (§29).
            Ok(Some(BTreeMap::from([(
                config.connection_string_argument.clone(),
                Value::String(secret.expose().to_owned()),
            )])))
        }
    }
}

#[cfg(test)]
mod tests {
    use async_trait::async_trait;
    use fabric_connector::{ConnectionName, ConnectorId, ResolvedSecret, SecretRef};

    use super::*;

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
        NdcConnectorConfig {
            id: ConnectorId::try_new("postgres").unwrap(),
            endpoint: "http://connector:8080".to_owned(),
            timeout_seconds: 10,
            connection_name_argument: "connection_name".to_owned(),
            connection_string_argument: "connection_string".to_owned(),
            procedures: BTreeMap::new(),
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
    async fn a_secret_connection_with_no_resolver_is_an_error_not_a_default_connection() {
        // Falling back to the default connection here would run one tenant's
        // query against whatever database the connector happens to point at.
        let selector = ConnectionSelector::Secret {
            reference: SecretRef::new("tenant/acme/data-primary"),
        };

        let error = request_arguments(&config(), &selector, None).await.unwrap_err();

        assert!(matches!(error, ConnectorError::InvalidOperation(_)));
    }
}
