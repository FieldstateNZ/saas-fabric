//! Waiting for the process to be asked to stop.

/// Resolves when the process should shut down.
///
/// Handles both `SIGINT` (a developer pressing `Ctrl-C`) and `SIGTERM` (an
/// orchestrator draining a pod). Without the second, a pod would be `SIGKILL`ed
/// after its grace period with in-flight requests dropped.
pub async fn shutdown_signal() {
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
