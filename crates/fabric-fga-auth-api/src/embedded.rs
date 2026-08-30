//! Starting the authorization service this process fronts, and outliving it
//! by exactly zero seconds.
//!
//! # Why this process owns it rather than a supervisor script
//!
//! Two processes in one container need somebody to decide what happens when
//! one of them dies. A shell that starts both and waits leaves the common case
//! wrong: the authorization service exits, the front keeps serving, every
//! decision answers `503`, and the container stays *healthy* because the
//! process that liveness asks about is still running. An operator sees a
//! working pod that authorizes nothing.
//!
//! So the front owns the child. If the service exits for any reason, this
//! process exits too and lets the orchestrator do what it is for.
//!
//! # Why it is bound to loopback here and not in configuration
//!
//! The address is built from a port. There is no argument, environment
//! variable or file entry that can move it off `127.0.0.1`, because a service
//! running with no authentication of its own is safe only for as long as
//! nothing outside the container can reach it (ADR 0016).

use std::time::Duration;

use tokio::process::{Child, Command};

use crate::config::Embedded;

/// Starts the authorization service and waits for it to answer.
///
/// # Errors
///
/// Returns a message if it could not be spawned or did not become ready
/// within the configured window.
pub async fn start(config: &Embedded) -> Result<Child, String> {
    let probe = reqwest::Client::builder()
        .timeout(Duration::from_secs(2))
        .build()
        .map_err(|error| format!("readiness client: {error}"))?;

    // Spent before the child exists, deliberately. An HTTP client does one-off
    // work on its **first request** -- loading the platform's trust store --
    // and some platforms make that seconds of *blocking* work inside a poll,
    // which stalls the very task that is supposed to be watching the child.
    // The race below is then not a race, and a service that died in the first
    // millisecond is noticed only when the stall ends.
    //
    // Measured on a developer machine: first request 15s, second 298µs, raw
    // TCP connect 165µs. Fast on the deployment target, where the trust store
    // is one small file — which is exactly why it would never have shown up
    // there and would have stayed a mystery here.
    let _ = probe
        .get(format!("http://127.0.0.1:{}/healthz", config.port))
        .send()
        .await;

    let http = format!("127.0.0.1:{}", config.port);
    let grpc = format!("127.0.0.1:{}", config.port.saturating_add(1));

    let mut child = Command::new(&config.binary)
        .arg("run")
        .args(["--http-addr", &http])
        .args(["--grpc-addr", &grpc])
        // It has no authentication of its own, and needs none: nothing outside
        // this container can reach either address.
        .args(["--authn-method", "none"])
        .args(datastore_args(config))
        .kill_on_drop(true)
        .spawn()
        .map_err(|error| format!("could not start {}: {error}", config.binary))?;

    wait_until_ready(config, &mut child, &probe).await?;

    Ok(child)
}

/// The datastore arguments, if a deployment stated one.
///
/// The URI carries a credential and is passed as an argument rather than
/// logged. Nothing here prints it.
fn datastore_args(config: &Embedded) -> Vec<String> {
    config.datastore.as_ref().map_or_else(Vec::new, |datastore| {
        vec![
            "--datastore-engine".to_owned(),
            datastore.engine.clone(),
            "--datastore-uri".to_owned(),
            datastore.uri.clone(),
        ]
    })
}

/// Waits for the service to answer, racing that against it dying instead.
///
/// A race rather than an interleaved poll. Checking the child between health
/// probes notices an exit only at the next boundary, which is however long the
/// probe took — measured at nearly thirteen seconds for a service that had
/// already gone. Racing notices it as it happens.
async fn wait_until_ready(
    config: &Embedded,
    child: &mut tokio::process::Child,
    probe: &reqwest::Client,
) -> Result<(), String> {
    tokio::select! {
        exited = child.wait() => Err(match exited {
            Ok(status) => format!(
                "the authorization service exited during startup ({status}); \
                 it was asked to listen on 127.0.0.1:{}",
                config.port
            ),
            Err(error) => format!("the authorization service could not be waited on: {error}"),
        }),

        ready = poll_until_ready(config, probe) => ready,
    }
}

/// Polls the health endpoint until it answers or the window closes.
async fn poll_until_ready(config: &Embedded, probe: &reqwest::Client) -> Result<(), String> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(config.start_timeout_seconds);
    let url = format!("http://127.0.0.1:{}/healthz", config.port);

    loop {
        let answered = probe
            .get(&url)
            .send()
            .await
            .is_ok_and(|response| response.status().is_success());

        if answered {
            return Ok(());
        }

        if tokio::time::Instant::now() >= deadline {
            return Err(format!(
                "the authorization service did not answer on 127.0.0.1:{} within {}s",
                config.port, config.start_timeout_seconds
            ));
        }

        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}
