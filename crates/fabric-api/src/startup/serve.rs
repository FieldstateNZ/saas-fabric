//! Binding the listeners, serving until the process is asked to stop, and
//! stopping the background tasks.

use std::future::IntoFuture as _;

use tokio::net::TcpListener;
use tokio::sync::watch;

use crate::startup::{shutdown_signal, Application};

/// Binds the main listener and, only when one is configured, the
/// reader-generation listener; serves both until a shutdown signal; then
/// stops the background tasks.
///
/// The reader-generation listener stops with the main one. A failure of its
/// own is logged and does not stop the main listener: it reports on serving,
/// and is not part of it.
///
/// # Errors
///
/// A message when a listener cannot be bound, or the main server fails.
pub async fn serve(application: Application, token_mode: &str) -> Result<(), String> {
    let Application {
        router,
        listen,
        reader_generation,
        tasks,
    } = application;
    let bound = bind(&listen).await;
    let reader = match &reader_generation {
        Some(reader) => bind(&reader.address).await.map(Some),
        None => Ok(None),
    };
    let (main, reader) = match (bound, reader) {
        (Ok(main), Ok(reader)) => (main, reader),
        (Err(error), _) | (_, Err(error)) => {
            tasks.shutdown().await;
            return Err(error);
        }
    };
    tracing::info!(
        event = "fabric.listening",
        address = listen,
        token_mode,
        "runtime plane listening"
    );

    let (stop, stopped) = watch::channel(false);
    let reader = reader.zip(reader_generation).map(|(listener, served)| {
        tracing::info!(
            event = "fabric.reader_generation_listening",
            address = served.address,
            "serving this replica's generation on its own listener"
        );
        let mut stopped = stopped.clone();
        tokio::spawn(
            axum::serve(listener, served.router)
                .with_graceful_shutdown(async move {
                    let _ = stopped.wait_for(|stop| *stop).await;
                })
                .into_future(),
        )
    });

    let result = axum::serve(main, router)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .map_err(|error| format!("server error: {error}"));

    let _ = stop.send(true);
    if let Some(reader) = reader {
        if let Ok(Err(error)) = reader.await {
            tracing::error!(event = "fabric.reader_generation_failed", reason = %error);
        }
    }
    tasks.shutdown().await;
    result
}

async fn bind(address: &str) -> Result<TcpListener, String> {
    TcpListener::bind(address)
        .await
        .map_err(|error| format!("could not bind {address}: {error}"))
}
