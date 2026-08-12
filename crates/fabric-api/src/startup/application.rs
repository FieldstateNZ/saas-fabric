//! The application graph, top to bottom.

use std::sync::Arc;

use axum::Router;
use fabric_connector::SecretResolver;
use fabric_data_api::build_data_api;
use fabric_identity::build_identity;
use fabric_tenant_runtime::{build_runtime, JsonFileSource, RuntimeHandles};
use tower_http::trace::TraceLayer;

use crate::config::AppConfig;
use crate::health::{health_routes, HealthState};
use crate::secrets::EnvSecretResolver;
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
}

/// Wires every domain. **The whole application graph is this function.**
///
/// # Errors
///
/// Returns a message from whichever step failed. Every one of them is fatal at
/// startup by design: a process that cannot load its identity keys, its
/// reconciled state, its connectors, or its catalogue can serve no tenant, and
/// failing here surfaces the problem where a deployment pipeline catches it.
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

    // 4. Connectors. Each negotiates capabilities and schema once, here.
    let connectors = connectors::build(config, &secrets).await?;

    // 5. The Data API.
    let data = build_data_api(
        &config.data_api,
        catalog::load(&config.catalog_path)?,
        config.permissions.clone(),
        Arc::clone(&runtime),
        connectors.clone(),
        identity,
    )?;

    // 6. The HTTP surface. Cross-cutting concerns are middleware applied once,
    //    never repeated per handler.
    let router = Router::new()
        .nest("/data", data)
        .merge(health_routes(HealthState { runtime, connectors }))
        .layer(TraceLayer::new_for_http());

    Ok(Application {
        router,
        listen: config.listen.clone(),
        refresh,
    })
}
