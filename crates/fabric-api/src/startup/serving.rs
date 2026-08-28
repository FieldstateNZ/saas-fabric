//! Everything between the reconciled state and the served router.
//!
//! Split out of [`build`](super::build) so the connector retry loop has a
//! scope with an edge to it. The loop is spawned partway through, and two
//! steps that can fail come after it; keeping them in one function whose only
//! failure exit stops the loop is what makes "a failed build leaves nothing
//! running" a property of the shape rather than of remembering.

use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use fabric_connector::{ConnectorRegistry, SecretResolver};
use fabric_data_api::build_data_api;
use fabric_identity::IdentityResolver;
use fabric_tenant_runtime::RuntimeResolver;

use crate::config::AppConfig;
use crate::health::{health_routes, HealthState};
use crate::secrets::EnvSecretResolver;
use crate::startup::background_tasks::stop_connector_retry;
use crate::startup::compose::compose;
use crate::startup::connectors::ConnectorRetryHandle;
use crate::startup::{catalog, connectors};

/// Negotiates the connectors, builds the Data API, and composes the surface.
///
/// # Errors
///
/// A message from whichever step failed. The connector retry loop is stopped
/// before returning one, since it is spawned before the last two steps run.
pub(super) async fn build(
    config: &AppConfig,
    runtime: &Arc<RuntimeResolver>,
    identity: &Arc<IdentityResolver>,
) -> Result<(Router, ConnectorRetryHandle), String> {
    // Secrets: one resolver shared by every connector.
    let secrets: Arc<dyn SecretResolver> = Arc::new(EnvSecretResolver);

    // Connectors. Each negotiates capabilities and schema once, here. A
    // connector that fails is retried in the background rather than aborting
    // startup — see `startup::connectors` for the policy (§35).
    let (connectors, connector_retry) = connectors::build(config, &secrets).await?;

    match router(config, runtime, identity, connectors) {
        Ok(router) => Ok((router, connector_retry)),
        Err(error) => {
            stop_connector_retry(connector_retry).await;
            Err(error)
        }
    }
}

/// Builds the Data API and the probes, and joins them.
fn router(
    config: &AppConfig,
    runtime: &Arc<RuntimeResolver>,
    identity: &Arc<IdentityResolver>,
    connectors: ConnectorRegistry,
) -> Result<Router, String> {
    let data = build_data_api(
        &config.data_api,
        catalog::load(&config.catalog_path)?,
        config.permissions.clone(),
        Arc::clone(runtime),
        connectors.clone(),
        Arc::clone(identity),
    )?;

    // The probes read the same resolver and the same administrator role the
    // Data API authorises against, rather than a second opinion about either.
    let health = health_routes(HealthState {
        runtime: Arc::clone(runtime),
        connectors,
        identity: Arc::clone(identity),
        administrator_role: config.permissions.administrator_role.clone(),
    });

    Ok(compose(
        data,
        health,
        Duration::from_secs(config.request_timeout_seconds),
    ))
}
