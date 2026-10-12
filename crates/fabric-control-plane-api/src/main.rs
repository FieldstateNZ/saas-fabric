//! The SaaS Fabric control plane host.

use fabric_control_plane_api::config::{ControlPlaneAppConfig, CONFIG_PATH_VAR};
use fabric_control_plane_api::{contracts, recovery, startup, telemetry};
use std::io::Write as _;

#[tokio::main]
async fn main() -> std::process::ExitCode {
    // Before logging, configuration or anything that could touch the network:
    // a release runs this on a bare image with no configuration mounted.
    if contracts::requested(std::env::args().skip(1)) {
        return match writeln!(std::io::stdout(), "{}", contracts::report()) {
            Ok(()) => std::process::ExitCode::SUCCESS,
            Err(_) => std::process::ExitCode::FAILURE,
        };
    }

    // The recovery command, also before configuration: an operator runs it
    // against the published files or namespace while publication is blocked.
    if recovery::requested(std::env::args().skip(1)) {
        return recover().await;
    }

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

/// Runs `fabric-control-plane-api held-publication ...` and prints its
/// one-line report.
async fn recover() -> std::process::ExitCode {
    let action = match recovery::parse(std::env::args().skip(1)) {
        Ok(action) => action,
        Err(usage) => {
            let _ = writeln!(std::io::stderr(), "{usage}");
            return std::process::ExitCode::from(2);
        }
    };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs());
    let outcome = recovery::run(action, now).await;
    match writeln!(std::io::stdout(), "{}", outcome.report) {
        Ok(()) => std::process::ExitCode::from(outcome.status),
        Err(_) => std::process::ExitCode::from(2),
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

    // There is no reconciliation task to stop any more. Convergence happens
    // inside an operator's request, or in a task spawned by one, so shutting
    // the server down is the whole of shutting the process down (ADR 0012).

    result
}
