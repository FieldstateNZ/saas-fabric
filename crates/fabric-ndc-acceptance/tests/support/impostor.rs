//! Starts a real HTTP process that is not an NDC connector.
//!
//! `nginx`, answering `200` with its default HTML page, stands in for "a
//! connector endpoint that answers HTTP but is not NDC" -- the case that
//! distinguishes "the database refused this query" from "this is not an NDC
//! connector at all" in `fabric_connector_ndc::client::response_decoding`
//! (`m2-ndc/plan.md` §3.6). This slice only starts it and hands back its
//! base URL; the acceptance test that points a connector configuration at
//! it and asserts the refusal is a later slice of issue #62.

use std::time::Duration;

use crate::support::docker;
use crate::support::images;
use crate::support::names::RunId;

/// How long to wait for nginx to accept connections before giving up.
const READY_DEADLINE: Duration = Duration::from_secs(15);

/// Starts nginx on the run's network and waits for it to answer.
///
/// Returns the container and the base URL of its ephemeral host port.
pub fn start(run_id: &RunId, network: &str) -> (docker::Container, String) {
    let container = docker::run(&docker::RunSpec {
        name: run_id.container_name("nginx"),
        image: images::NGINX.to_owned(),
        network: network.to_owned(),
        env: Vec::new(),
        publish: Some(80),
        mount_ro: None,
        command: Vec::new(),
    })
    .unwrap_or_else(|error| panic!("could not start nginx: {error}"));

    let host_port = docker::port(&container, 80)
        .unwrap_or_else(|error| panic!("could not read nginx's published port: {error}"));

    wait_ready(&container);

    (container, format!("http://127.0.0.1:{host_port}"))
}

/// Unlike the distroless connector image, this one has a shell and `wget`
/// (busybox, built into the alpine base) -- enough to prove nginx is
/// actually answering rather than merely running.
fn wait_ready(container: &docker::Container) {
    let ready = docker::poll_until(READY_DEADLINE, || {
        docker::exec(
            container,
            &["wget", "-q", "-O", "/dev/null", "http://127.0.0.1:80/"],
        )
        .is_ok_and(|output| output.status.success())
    });
    assert!(ready, "nginx did not become ready within {READY_DEADLINE:?}");
}
