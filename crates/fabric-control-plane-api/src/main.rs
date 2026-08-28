//! The SaaS Fabric control plane host.

use fabric_control_plane_api::config::{ControlPlaneAppConfig, CONFIG_PATH_VAR};
use fabric_control_plane_api::{startup, telemetry};

#[tokio::main]
async fn main() -> std::process::ExitCode {
    telemetry::init();

    let path = std::env::args()
        .nth(1)
        .or_else(|| std::env::var(CONFIG_PATH_VAR).ok())
        .unwrap_or_else(|| "/etc/fabric/control-plane.toml".to_owned());

    match run(&path).await {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            // Rust has no fatal log level: an unrecoverable startup condition
            // logs at error and exits non-zero. It does not panic.
            tracing::error!(
                event = "control_plane.startup_failed",
                reason = %error,
                "the control plane could not start"
            );

            std::process::ExitCode::FAILURE
        }
    }
}

/// Builds and serves the control plane.
async fn run(config_path: &str) -> Result<(), String> {
    let config = ControlPlaneAppConfig::load(config_path)?;
    let application = startup::build(&config).await?;

    let listener = tokio::net::TcpListener::bind(&application.listen)
        .await
        .map_err(|error| format!("could not bind {}: {error}", application.listen))?;

    tracing::info!(
        event = "control_plane.listening",
        address = application.listen,
        "control plane listening"
    );

    let result = axum::serve(listener, application.router)
        .with_graceful_shutdown(startup::shutdown_signal())
        .await
        .map_err(|error| format!("server error: {error}"));

    // A panicked reconciliation task is worth recording and is not worth
    // failing the shutdown over — the process is stopping either way, and the
    // exit status should report what the server did.
    if let Err(error) = application.reconciliation.shutdown().await {
        tracing::error!(
            event = "control_plane.reconciliation_task_failed",
            reason = %error,
            "the reconciliation loop did not stop cleanly"
        );
    }

    result
}
