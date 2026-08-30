//! The Fabric authorization front, and the service it fronts.

use fabric_fga_auth_api::config::{AppConfig, CONFIG_PATH_VAR};
use fabric_fga_auth_api::{embedded, startup, telemetry};

#[tokio::main]
async fn main() -> std::process::ExitCode {
    telemetry::init();

    let path = std::env::args()
        .nth(1)
        .or_else(|| std::env::var(CONFIG_PATH_VAR).ok())
        .unwrap_or_else(|| "/etc/fabric/authorization.toml".to_owned());

    match run(&path).await {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            // No fatal log level in Rust: an unrecoverable condition logs at
            // error and exits non-zero rather than panicking.
            tracing::error!(
                event = "authorization_front.startup_failed",
                reason = %error,
                "the authorization front could not start"
            );

            std::process::ExitCode::FAILURE
        }
    }
}

/// Starts the authorization service, then serves in front of it.
async fn run(config_path: &str) -> Result<(), String> {
    let config = AppConfig::load(config_path)?;

    if config.embedded.datastore.is_none() {
        // Loud, because the failure it causes is silent: everything works
        // until a restart, and then every store, model and tuple is gone.
        tracing::warn!(
            event = "authorization_front.ephemeral_state",
            "no datastore configured: the authorization service will keep its \
             state in memory and lose it on restart"
        );
    }

    // Started first and deliberately: a front that came up without it would
    // answer `503` to everything while looking perfectly healthy.
    let mut service = embedded::start(&config.embedded).await?;

    tracing::info!(
        event = "authorization_front.embedded_ready",
        port = config.embedded.port,
        "the authorization service is answering on loopback"
    );

    let router = startup::build(&config)?;
    let listener = tokio::net::TcpListener::bind(&config.listen)
        .await
        .map_err(|error| format!("could not bind {}: {error}", config.listen))?;

    tracing::info!(
        event = "authorization_front.listening",
        address = %config.listen,
        "the authorization front is listening"
    );

    // Three ways this ends, and the middle one is the reason this process
    // supervises the service at all: if it dies, we die, and the orchestrator
    // restarts a pod rather than leaving one that authorizes nothing.
    tokio::select! {
        served = axum::serve(listener, router) => {
            served.map_err(|error| format!("the front stopped serving: {error}"))
        }

        exited = service.wait() => {
            Err(match exited {
                Ok(status) => format!("the authorization service exited ({status})"),
                Err(error) => format!("the authorization service could not be waited on: {error}"),
            })
        }

        () = shutdown() => {
            tracing::info!(event = "authorization_front.shutdown", "stopping");
            Ok(())
        }
    }
}

/// Resolves when the orchestrator asks this process to stop.
async fn shutdown() {
    let _ = tokio::signal::ctrl_c().await;
}
