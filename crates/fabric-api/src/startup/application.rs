//! The application graph, top to bottom.

use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use fabric_connector::SecretResolver;
use fabric_data_api::build_data_api;
use fabric_identity::build_identity;
use fabric_tenant_runtime::{build_runtime, JsonFileSource, RuntimeHandles};
use http::StatusCode;
use tower_http::timeout::TimeoutLayer;
use tower_http::trace::TraceLayer;

use crate::config::AppConfig;
use crate::health::{health_routes, HealthState};
use crate::secrets::EnvSecretResolver;
use crate::startup::connectors::ConnectorRetryHandle;
use crate::startup::{catalog, connectors, token_reader};

/// The assembled application, plus the handles that must outlive it.
pub struct Application {
    /// The HTTP surface.
    pub router: Router,

    /// The address to bind.
    pub listen: String,

    /// Background refreshers. Held so they can be stopped on shutdown;
    /// dropping them would orphan the tasks.
    pub refresh: RuntimeHandles,

    /// The connector retry loop (§35). A connector that failed startup
    /// negotiation is retried here in the background; held for the same
    /// reason as `refresh` — dropping it would orphan the task rather than
    /// stop it.
    pub connector_retry: ConnectorRetryHandle,
}

/// Wires every domain. **The whole application graph is this function.**
///
/// # Errors
///
/// Returns a message from whichever step failed. A process that cannot load
/// its identity keys, its reconciled state, or its catalogue can serve no
/// tenant, and failing here surfaces the problem where a deployment pipeline
/// catches it.
///
/// Connectors are the one exception to "any one failure is fatal": §35
/// tolerates a connector that cannot be negotiated, as long as at least one
/// other connector can. See the `connectors` submodule for the policy.
pub async fn build(config: &AppConfig) -> Result<Application, String> {
    // 1. Identity. First, because it decides what the process will believe.
    let identity = build_identity(config.identity.clone(), token_reader::build(&config.token)?)?;

    // 2. Runtime state. Two independently reconciled resources, read from files
    //    a controller writes. Never queries Git or Kubernetes (§6).
    let (runtime, refresh) = build_runtime(
        &config.tenant_runtime,
        Arc::new(JsonFileSource::new(&config.tenants_path)),
        Arc::new(JsonFileSource::new(&config.data_sources_path)),
    )
    .await?;

    // 3. Secrets. One resolver shared by every connector.
    let secrets: Arc<dyn SecretResolver> = Arc::new(EnvSecretResolver);

    // 4. Connectors. Each negotiates capabilities and schema once, here. A
    //    connector that fails is retried in the background rather than
    //    aborting startup — see `startup::connectors` for the policy (§35).
    let (connectors, connector_retry) = connectors::build(config, &secrets).await?;

    // 5. The Data API, wrapped in its own request-timeout budget — the
    //    outermost of three timeout scopes; see
    //    `AppConfig::request_timeout_seconds` for the other two and the
    //    relationship between them (§36). Applied to `data` directly, before
    //    it is nested below, so the scope stays correct no matter where it
    //    ends up mounted.
    let data = build_data_api(
        &config.data_api,
        catalog::load(&config.catalog_path)?,
        config.permissions.clone(),
        Arc::clone(&runtime),
        connectors.clone(),
        identity,
    )?
    .layer(TimeoutLayer::with_status_code(
        StatusCode::GATEWAY_TIMEOUT,
        Duration::from_secs(config.request_timeout_seconds),
    ));

    // 6. The HTTP surface. Cross-cutting concerns are middleware applied once,
    //    never repeated per handler.
    //
    //    The Data API router carries its whole path itself — `/v1/data/...`,
    //    see `fabric_data_api::API_PREFIX` — so it is merged rather than
    //    nested. Nesting a `/data` prefix here on top of a router that
    //    already knows its version would produce `/data/v1/...`, with the
    //    version buried a segment deep.
    let router = Router::new()
        .merge(data)
        .merge(health_routes(HealthState { runtime, connectors }))
        .layer(TraceLayer::new_for_http());

    Ok(Application {
        router,
        listen: config.listen.clone(),
        refresh,
        connector_retry,
    })
}
