//! The SaaS Fabric runtime plane host.
//!
//! Deliberately thin: load configuration, build the application, serve it, stop
//! cleanly. The application graph itself lives in
//! [`fabric_api::startup::build`], where it can be read — and tested — on its
//! own.

use fabric_api::config::{AppConfig, CONFIG_PATH_VAR};
use fabric_api::{startup, telemetry};

#[tokio::main]
async fn main() -> std::process::ExitCode {
    telemetry::init();

    // `CONFIG_PATH_VAR` rather than a literal: it names the file to load and is
    // deliberately *not* a setting, which only holds if the constant the
    // settings namespace is tested against is the one actually read here.
    let path = std::env::args()
        .nth(1)
        .or_else(|| std::env::var(CONFIG_PATH_VAR).ok())
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

    let application = startup::build(&config).await?;

    let listener = tokio::net::TcpListener::bind(&application.listen)
        .await
        .map_err(|error| format!("could not bind {}: {error}", application.listen))?;

    tracing::info!(
        event = "fabric.listening",
        address = application.listen,
        token_mode = config.token.mode_name(),
        "runtime plane listening"
    );

    let result = axum::serve(listener, application.router)
        .with_graceful_shutdown(startup::shutdown_signal())
        .await
        .map_err(|error| format!("server error: {error}"));

    // Stop the background tasks before returning, so the process does not
    // linger with anything still polling.
    application.tasks.shutdown().await;

    result
}
