//! The application graph, top to bottom.

use std::sync::Arc;

use axum::Router;
use fabric_data_api::ResourceCatalogDecoder;
use fabric_identity::build_identity;
use fabric_tenant_runtime::{build_published_runtime, LastKnownGoodStore, PublishedFiles, PublishedRuntime};

use crate::config::AppConfig;
use crate::reader_generation::ReaderGenerationListener;
use crate::startup::background_tasks::{stop_refreshers, BackgroundTasks};
use crate::startup::{serving, token_reader};

/// The assembled application, plus the tasks that must outlive it.
pub struct Application {
    /// The HTTP surface.
    pub router: Router,

    /// The address to bind.
    pub listen: String,

    /// The separate listener serving this replica's served generation, only
    /// when `reader_generation_listen` is set (G7).
    pub reader_generation: Option<ReaderGenerationListener>,

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

    // 2. Runtime state. Tenants, data sources and the catalogue, read from
    //    files a controller writes, and installed only as the complete
    //    generation the publication record beside the tenants file names, the
    //    catalogue decoded by the Data API's own decoder. Never queries Git or
    //    Kubernetes (§6). Every request reads one installed view of all three,
    //    and a refresh installs the next whole or not at all (G8). This is the
    //    step that starts background work, so from here on a failure has
    //    something to clean up.
    //    With a last-known-good directory configured, each installed
    //    generation is also persisted there, and a start with no complete
    //    published generation primes from it instead of failing (G5).
    let published = PublishedFiles::new(
        &config.tenants_path,
        &config.data_sources_path,
        &config.catalog_path,
        Arc::new(ResourceCatalogDecoder),
    );
    let published = match &config.tenant_runtime.last_known_good_dir {
        Some(directory) => published.with_last_known_good(LastKnownGoodStore::new(directory)),
        None => published,
    };
    let PublishedRuntime {
        resolver: runtime,
        handles: refresh,
        status,
    } = build_published_runtime(&config.tenant_runtime, Arc::new(published)).await?;

    let reader_generation = ReaderGenerationListener::configured(config, &runtime);
    match serving::build(config, &runtime, &identity, status).await {
        Ok((router, connector_retry)) => Ok(Application {
            router,
            listen: config.listen.clone(),
            reader_generation,
            tasks: BackgroundTasks::new(refresh, connector_retry),
        }),
        Err(error) => {
            stop_refreshers(refresh).await;
            Err(error)
        }
    }
}
