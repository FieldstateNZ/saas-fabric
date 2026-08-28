//! Waiting for a signal to stop.

/// Resolves when the process is asked to stop.
///
/// Both signals, because the two arrive from different places: `SIGTERM` from
/// a container runtime beginning a rolling update, `SIGINT` from a developer's
/// terminal. Handling only one would make `cargo run` unstoppable or a
/// deployment's shutdown a kill.
///
/// The graceful stop matters more here than it looks: a reconciliation sweep
/// that is interrupted mid-apply leaves a realm partly converged, which is
/// safe — every action is additive and idempotent, so the next pass continues
/// from what exists — but the *status* would be left saying `pending` with
/// nothing running to change it. Letting the sweep finish avoids that on the
/// ordinary path.
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
            // Nothing useful to do if the handler cannot be installed: the
            // process still stops on `SIGINT`, and panicking during startup
            // over a signal handler would be a worse outcome than a slower
            // shutdown.
            Err(error) => {
                tracing::warn!(
                    event = "control_plane.signal_handler_unavailable",
                    reason = %error,
                    "could not listen for SIGTERM; shutdown will not be graceful"
                );
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
}
