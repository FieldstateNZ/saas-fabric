//! The application graph, top to bottom.

use std::sync::Arc;

use axum::Router;
use fabric_identity::build_identity;
use fabric_tenant_runtime::{build_runtime, JsonFileSource};

use crate::config::AppConfig;
use crate::startup::background_tasks::{stop_refreshers, BackgroundTasks};
use crate::startup::{serving, token_reader};

/// The assembled application, plus the tasks that must outlive it.
pub struct Application {
    /// The HTTP surface.
    pub router: Router,

    /// The address to bind.
    pub listen: String,

    /// The refreshers and the connector retry loop. Held so they can be
    /// stopped on shutdown; dropping them would orphan the tasks.
    pub tasks: BackgroundTasks,
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
///
/// Every failure after step 2 stops the refreshers before returning. See
/// the `background_tasks` submodule for why that is not left to the process
/// exiting.
pub async fn build(config: &AppConfig) -> Result<Application, String> {
    // 1. Identity. First, because it decides what the process will believe —
    //    including which issuer names which tenant. `build_identity` runs
    //    `IdentityConfig::validate`, so a deployment that has not stated
    //    `[identity].trusted_issuers` stops here rather than refusing every
    //    request later (ADR 0019 §2).
    let identity = build_identity(
        config.identity.clone(),
        token_reader::build(&config.token, config.leeway)?,
    )?;

    // 2. Runtime state. Two independently reconciled resources, read from files
    //    a controller writes. Never queries Git or Kubernetes (§6). This is the
    //    step that starts background work, so from here on a failure has
    //    something to clean up.
    let (runtime, refresh) = build_runtime(
        &config.tenant_runtime,
        Arc::new(JsonFileSource::new(&config.tenants_path)),
        Arc::new(JsonFileSource::new(&config.data_sources_path)),
    )
    .await?;

    match serving::build(config, &runtime, &identity).await {
        Ok((router, connector_retry)) => Ok(Application {
            router,
            listen: config.listen.clone(),
            tasks: BackgroundTasks::new(refresh, connector_retry),
        }),
        Err(error) => {
            stop_refreshers(refresh).await;
            Err(error)
        }
    }
}
