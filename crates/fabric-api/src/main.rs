//! The SaaS Fabric runtime plane host — the composition root.
//!
//! Everything the process trusts, connects to, and exposes is wired here, in
//! one file, in the order it happens. No dependency-injection container, no
//! assembly scanning, no registry macros: the entire application graph is
//! readable top to bottom in [`build`].
//!
//! That is worth the verbosity. In a system whose whole job is keeping tenants
//! apart, "what does this process actually trust?" should be answerable by
//! reading one function rather than by tracing attributes through six crates.

use std::sync::Arc;

use axum::Router;
use fabric_connector::{ConnectorRegistry, DataConnector, SecretResolver};
use fabric_connector_ndc::build_ndc_connector;
use fabric_data_api::{build_data_api, ResourceCatalog};
use fabric_identity::{
    build_identity, TokenReader, TrustedIngressReader, ValidatingReader, VerificationKeys,
};
use fabric_tenant_runtime::{build_tenant_runtime, FileBindingSource, RefreshHandle};
use tower_http::trace::TraceLayer;

use fabric_api::config::{AppConfig, TokenConfig};
use fabric_api::health::{health_routes, HealthState};
use fabric_api::secrets::EnvSecretResolver;
use fabric_api::telemetry;

/// The assembled application, plus the handles that must outlive it.
struct Application {
    router: Router,
    listen: String,
    /// Held so the background refresher can be stopped on shutdown. Dropping it
    /// would orphan the task.
    refresh: RefreshHandle,
}

#[tokio::main]
async fn main() -> std::process::ExitCode {
    telemetry::init();

    let path = std::env::args()
        .nth(1)
        .or_else(|| std::env::var("FABRIC_CONFIG").ok())
        .unwrap_or_else(|| "/etc/fabric/config.toml".to_owned());

    match run(&path).await {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            // Rust has no fatal log level: an unrecoverable startup condition
            // logs at error and exits non-zero. It does not panic.
            tracing::error!(
                event = "fabric.startup_failed",
                reason = %error,
                "the runtime plane could not start"
            );

            std::process::ExitCode::FAILURE
        }
    }
}

/// Builds and serves the application.
async fn run(config_path: &str) -> Result<(), String> {
    let config = AppConfig::load(config_path)?;
    config.validate()?;

    let application = build(&config).await?;

    let listener = tokio::net::TcpListener::bind(&application.listen)
        .await
        .map_err(|error| format!("could not bind {}: {error}", application.listen))?;

    tracing::info!(
        event = "fabric.listening",
        address = application.listen,
        "runtime plane listening"
    );

    let result = axum::serve(listener, application.router)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .map_err(|error| format!("server error: {error}"));

    // Stop the refresher before returning, so the process does not linger with
    // a background task still polling.
    if let Err(error) = application.refresh.shutdown().await {
        tracing::warn!(event = "fabric.refresher_shutdown_failed", reason = %error);
    }

    result
}

/// Wires every domain. **The whole application graph is this function.**
async fn build(config: &AppConfig) -> Result<Application, String> {
    // 1. Identity. First, because it decides what the process will believe.
    let identity = build_identity(config.identity.clone(), token_reader(&config.token)?)?;

    // 2. Tenant runtime. Reads bindings that reconciliation has already
    //    written; never queries Git or Kubernetes (§6).
    let source = Arc::new(FileBindingSource::new(&config.bindings_path));
    let (tenants, refresh) = build_tenant_runtime(&config.tenant_runtime, source).await?;

    // 3. Secrets. One resolver shared by every connector.
    let secrets: Arc<dyn SecretResolver> = Arc::new(EnvSecretResolver);

    // 4. Connectors. Each negotiates capabilities and schema once, here.
    let connectors = build_connectors(config, &secrets).await?;

    // 5. The Data API.
    let catalog = load_catalog(config)?;
    let data = build_data_api(
        &config.data_api,
        catalog,
        config.permissions.clone(),
        Arc::clone(&tenants),
        connectors.clone(),
        Arc::clone(&identity),
    )?;

    // 6. The HTTP surface. Cross-cutting concerns are middleware applied once,
    //    never repeated per handler.
    let router = Router::new()
        .nest("/data", data)
        .merge(health_routes(HealthState { tenants, connectors }))
        .layer(TraceLayer::new_for_http());

    Ok(Application {
        router,
        listen: config.listen.clone(),
        refresh,
    })
}

/// Builds the configured token reader.
///
/// Which reader runs is a security decision, so it is made here in the
/// composition root rather than inside the identity crate — a reader of this
/// file can see the deployed posture without going looking.
fn token_reader(config: &TokenConfig) -> Result<Arc<dyn TokenReader>, String> {
    match config {
        TokenConfig::Validating {
            jwks_path,
            issuers,
            audiences,
        } => {
            let document = std::fs::read_to_string(jwks_path)
                .map_err(|error| format!("could not read JWKS from {}: {error}", jwks_path.display()))?;

            let keys = VerificationKeys::from_jwks_json(&document)?;
            let mut reader = ValidatingReader::new(keys);

            if !issuers.is_empty() {
                reader = reader.with_issuers(issuers);
            }

            if !audiences.is_empty() {
                reader = reader.with_audiences(audiences);
            }

            Ok(Arc::new(reader))
        }

        TokenConfig::TrustedIngress => {
            // Loud on purpose. This posture is only sound while §9's network
            // controls hold, and that is not something the process can check.
            tracing::warn!(
                event = "fabric.token_signatures_not_verified",
                "running in trusted-ingress mode: bearer token signatures are NOT verified. \
                 This is only safe if network policy prevents untrusted access to this service."
            );

            Ok(Arc::new(TrustedIngressReader::new(
                fabric_core::SystemClock::shared(),
            )))
        }
    }
}

/// Negotiates every configured connector.
async fn build_connectors(
    config: &AppConfig,
    secrets: &Arc<dyn SecretResolver>,
) -> Result<ConnectorRegistry, String> {
    let mut registry = ConnectorRegistry::new();

    for connector in &config.connectors {
        let built = build_ndc_connector(connector.clone(), Some(Arc::clone(secrets))).await?;
        registry = registry.with(built as Arc<dyn DataConnector>);
    }

    Ok(registry)
}

/// Loads the resource catalogue.
fn load_catalog(config: &AppConfig) -> Result<ResourceCatalog, String> {
    let contents = std::fs::read_to_string(&config.catalog_path).map_err(|error| {
        format!(
            "could not read the resource catalogue from {}: {error}",
            config.catalog_path.display()
        )
    })?;

    serde_json::from_str(&contents).map_err(|error| {
        format!(
            "the resource catalogue at {} is malformed: {error}",
            config.catalog_path.display()
        )
    })
}

/// Resolves when the process is asked to stop.
async fn shutdown_signal() {
    let interrupt = async {
        let _ = tokio::signal::ctrl_c().await;
    };

    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut signal) => {
                signal.recv().await;
            }
            // Without SIGTERM handling the pod is eventually SIGKILLed, which
            // is worse but not fatal — so log and wait rather than give up.
            Err(error) => {
                tracing::warn!(event = "fabric.sigterm_unavailable", reason = %error);
                std::future::pending::<()>().await;
            }
        }
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = interrupt => {}
        () = terminate => {}
    }

    tracing::info!(event = "fabric.shutdown_requested", "shutting down");
}
