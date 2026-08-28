//! Negotiating the configured connectors, tolerating partial failure.
//!
//! # Policy (§35)
//!
//! One connector that cannot be reached at startup no longer takes the whole
//! data plane offline. [`negotiate::negotiate`] records it as unavailable —
//! logged at `error`, registered under its id as a `PendingConnector` that
//! fails every operation closed until it recovers — and hands it to
//! [`retry::spawn`], which keeps renegotiating it on an interval in the
//! background (see [`AppConfig::connector_retry_interval_seconds`]). Tenants
//! bound to any other, healthy connector are unaffected the entire time; a
//! request routed to the unavailable one gets
//! [`ConnectorError::Unreachable`](fabric_connector::ConnectorError::Unreachable)
//! instead of silently succeeding against the wrong backend or hanging.
//!
//! The process still refuses to start in the one case tolerance cannot help:
//! when *no* connector could be negotiated at all, this replica could serve
//! no tenant no matter what, and starting it would just turn every request
//! into a failure instead of surfacing the problem where a deployment
//! pipeline catches it.
//!
//! Readiness reflects the same policy rather than treating one failed
//! connector as reason to pull an otherwise-healthy replica out of rotation
//! — see [`crate::health`].

mod logging;
mod negotiate;
mod negotiation_failure;
mod pending_connector;
mod pending_connector_delegation;
mod retry;

#[cfg(test)]
mod pending_connector_tests;

use std::sync::Arc;
use std::time::Duration;

use fabric_connector::{ConnectorRegistry, SecretResolver};

pub use retry::ConnectorRetryHandle;

use crate::config::AppConfig;

/// Negotiates every configured connector and starts the background retry
/// loop for whichever ones failed.
///
/// # Errors
///
/// See the module docs: this fails only when not one configured connector
/// could be negotiated.
pub(super) async fn build(
    config: &AppConfig,
    secrets: &Arc<dyn SecretResolver>,
) -> Result<(ConnectorRegistry, ConnectorRetryHandle), String> {
    let (registry, pending) = negotiate::negotiate(config, secrets).await?;

    let retry = retry::spawn(
        pending,
        Arc::clone(secrets),
        Duration::from_secs(config.connector_retry_interval_seconds),
    );

    Ok((registry, retry))
}
