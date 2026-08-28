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
/// configured without a resolver, or if the configuration names no argument for
/// the routing this tenant needs. All three fail the request closed — there is
/// no fallback to a default connection, because a fallback would silently
/// execute one tenant's query against another tenant's database (§28).
pub(crate) async fn request_arguments(
    config: &NdcConnectorConfig,
    selector: &ConnectionSelector,
    secrets: Option<&Arc<dyn SecretResolver>>,
) -> Result<Option<BTreeMap<String, Value>>, ConnectorError> {
    match selector {
        // The connector serves one database; there is nothing to select.
        ConnectionSelector::Default => Ok(None),

        ConnectionSelector::Named { name } => {
            let argument = routing_argument(
                config,
                config.connection_name_argument.as_deref(),
                "by connection name",
                "connection_name_argument",
            )?;

            Ok(Some(BTreeMap::from([(
                argument.to_owned(),
                Value::String(name.to_string()),
            )])))
        }

        ConnectionSelector::Secret { reference } => {
            let argument = routing_argument(
                config,
                config.connection_string_argument.as_deref(),
                "by connection string",
                "connection_string_argument",
            )?;

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
                argument.to_owned(),
                Value::String(secret.expose().to_owned()),
            )])))
        }
    }
}

/// The argument name for one routing mode, or a refusal if none is configured.
///
/// A tenant routed through a connector whose configuration never named an
/// argument for that routing mode has nowhere to put it. Sending nothing would
/// leave the connector to fall back to whatever connection it was started with
/// — which is the cross-tenant read this whole path exists to prevent — so the
/// request is refused instead.
///
/// This is the one routing failure that cannot be caught at startup:
/// connectors are negotiated before any tenant is resolved, so nothing at boot
/// knows a tenant will arrive needing a mode the operator did not configure.
/// `registration::routing_arguments` catches the *other* direction at boot,
/// where a mode the configuration does name is one the connector cannot serve.
///
/// # Errors
///
/// [`ConnectorError::InvalidOperation`], naming the setting to add.
fn routing_argument<'a>(
    config: &NdcConnectorConfig,
    argument: Option<&'a str>,
    routing: &str,
    setting: &str,
) -> Result<&'a str, ConnectorError> {
    argument.ok_or_else(|| {
        ConnectorError::InvalidOperation(format!(
            "connector {} has a tenant routed {routing}, but no {setting} is configured, so there \
             is no request-level argument to carry the routing",
            config.id
        ))
    })
}
