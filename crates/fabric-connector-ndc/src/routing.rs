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
