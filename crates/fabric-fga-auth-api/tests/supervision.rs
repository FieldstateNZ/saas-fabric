//! What happens to this process when the service it fronts will not run.
//!
//! # Why these are worth writing down
//!
//! The failure this design exists to prevent is silent: a front that came up
//! without its authorization service answers `503` to everything while its
//! liveness probe passes, so an operator sees a healthy pod that authorizes
//! nothing. Every case here must end in a startup failure, never in a process
//! that serves.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use fabric_fga_auth_api::config::{Datastore, Embedded};

/// A configuration pointing at a given binary.
fn embedded(binary: &str, start_timeout_seconds: u64) -> Embedded {
    Embedded {
        // A port nothing is listening on, so readiness can only be satisfied
        // by the child actually starting and answering.
        port: 59_991,
        binary: binary.to_owned(),
        start_timeout_seconds,
        datastore: None,
    }
}

#[tokio::test]
async fn a_service_that_cannot_be_started_fails_immediately() {
    let error = fabric_fga_auth_api::embedded::start(&embedded("/nonexistent/openfga", 5))
        .await
        .expect_err("a missing binary must not be survivable");

    assert!(
        error.contains("could not start"),
        "the reason must name the spawn failure: {error}"
    );
}

#[tokio::test]
async fn a_service_that_exits_is_noticed_rather_than_waited_out() {
    // `true` accepts any arguments and exits 0 immediately, which is exactly
    // the shape of a service that dies on startup.
    let binary = ["/usr/bin/true", "/bin/true"]
        .into_iter()
        .find(|path| std::path::Path::new(path).exists())
        .expect("a platform with true(1)");

    // A start timeout far longer than this test could tolerate. If the exit
    // were being waited out rather than noticed, this would take five minutes.
    //
    // Asserted this way rather than against a small elapsed time, because a
    // client's first request does one-off platform work -- seconds of it on
    // some developer machines, milliseconds on the deployment target. Timing
    // the whole call would make this test a measurement of a trust store.
    let generous = 300;

    let started = std::time::Instant::now();
    let error = fabric_fga_auth_api::embedded::start(&embedded(binary, generous))
        .await
        .expect_err("a service that exits must not be survivable");

    assert!(
        error.contains("exited during startup"),
        "the reason must say it exited, not that it timed out: {error}"
    );
    assert!(
        started.elapsed() < std::time::Duration::from_secs(generous / 4),
        "the exit must be noticed rather than waited out: took {:?} of a {generous}s window",
        started.elapsed()
    );
}

#[test]
fn a_datastore_uri_is_never_rendered_by_a_debug_format() {
    // The URI carries a credential. `Debug` is how a configuration ends up in
    // a log by accident, so the derive must not be the thing that leaks it.
    let datastore = Datastore {
        engine: "postgres".to_owned(),
        uri: "postgres://fabric:super-secret@db.internal/openfga".to_owned(),
    };

    let rendered = format!("{datastore:?}");

    assert!(
        !rendered.contains("super-secret"),
        "a debug format must not reveal the datastore credential: {rendered}"
    );
}
