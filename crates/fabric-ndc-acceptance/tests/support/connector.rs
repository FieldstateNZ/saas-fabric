//! Starts `ndc-postgres` on the run's network, in either configuration mode.

use std::path::PathBuf;
use std::time::Duration;

use crate::support::docker;
use crate::support::images;
use crate::support::names::RunId;
use crate::support::postgres;

/// Which of the two checked-in connector configurations to mount.
///
/// See `tests/fixtures/ndc-postgres-v3.1.0/README.md` for how both were
/// generated and why neither is hand-written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectorMode {
    /// `connectionSettings.connectionUri` only -- declares no request-level
    /// arguments. Used for the version/schema smoke test and the
    /// startup-refusal proof.
    Static,
    /// `dynamicSettings.mode = "named"`, `fallbackToStatic: false`, one
    /// named connection -- declares `connection_name` as a request-level
    /// argument for both queries and mutations.
    Named,
}

/// The fixture checked in for [`ConnectorMode::Static`].
const STATIC_CONFIG: &str = include_str!("../fixtures/ndc-postgres-v3.1.0/configuration-static.json");
/// The fixture checked in for [`ConnectorMode::Named`].
const NAMED_CONFIG: &str = include_str!("../fixtures/ndc-postgres-v3.1.0/configuration-named.json");

/// How long to wait for the connector's health check before giving up.
const HEALTHY_DEADLINE: Duration = Duration::from_secs(30);

/// Starts `ndc-postgres` against `pg`, in `mode`, and waits for it to report
/// healthy.
///
/// Returns the container, the base URL of its ephemeral host port, and the
/// directory its configuration was written into -- the caller owns removing
/// that directory once the container no longer needs it.
///
/// # The configuration placeholder
///
/// Neither fixture embeds a connection string: both name
/// `connectionSettings.connectionUri.variable` (and, in
/// [`ConnectorMode::Named`], `connectionSettings.dynamicSettings.connectionUris.map.*.variable`)
/// as `CONNECTION_URI` -- the connector reads the physical connection from
/// that environment variable at container start, not from the JSON. So the
/// only "template rewriting" this harness does is assembling that one
/// variable fresh per run, from the run's own postgres container's name
/// (its Docker network DNS name), user, password, and database -- see
/// [`postgres::connection_uri`] -- and passing it as `-e CONNECTION_URI=...`
/// when the container starts. The fixture's bytes are never rewritten.
pub fn start(
    run_id: &RunId,
    network: &str,
    pg: &docker::Container,
    mode: ConnectorMode,
) -> (docker::Container, String, PathBuf) {
    let config_dir = write_config_dir(run_id, mode);

    let container = docker::run(&docker::RunSpec {
        name: run_id.container_name("connector"),
        image: images::NDC_POSTGRES.to_owned(),
        network: network.to_owned(),
        env: vec![("CONNECTION_URI".to_owned(), postgres::connection_uri(pg))],
        publish: Some(8080),
        mount_ro: Some((config_dir.clone(), "/etc/connector".to_owned())),
        command: vec!["serve".to_owned()],
    })
    .unwrap_or_else(|error| panic!("could not start the connector: {error}"));

    wait_healthy(&container);

    let host_port = docker::port(&container, 8080)
        .unwrap_or_else(|error| panic!("could not read the connector's published port: {error}"));

    (container, format!("http://127.0.0.1:{host_port}"), config_dir)
}

/// Writes the checked-in fixture for `mode` to a fresh directory as
/// `configuration.json` -- the one filename `ndc-postgres` reads from
/// `HASURA_CONFIGURATION_DIRECTORY` -- and returns that directory.
fn write_config_dir(run_id: &RunId, mode: ConnectorMode) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("{}-config", run_id.container_name("connector")));
    std::fs::create_dir_all(&dir)
        .unwrap_or_else(|error| panic!("could not create {}: {error}", dir.display()));

    let contents = match mode {
        ConnectorMode::Static => STATIC_CONFIG,
        ConnectorMode::Named => NAMED_CONFIG,
    };
    std::fs::write(dir.join("configuration.json"), contents).unwrap_or_else(|error| {
        panic!(
            "could not write configuration.json into {}: {error}",
            dir.display()
        )
    });

    dir
}

/// Polls the image's own `/bin/ndc-postgres check-health` -- a
/// no-shell-required liveness probe the image ships for exactly this,
/// since the image is distroless and has no `sh` a `docker exec ... sh -c`
/// health poll could use.
fn wait_healthy(container: &docker::Container) {
    let healthy = docker::poll_until(HEALTHY_DEADLINE, || {
        docker::exec(
            container,
            &["/bin/ndc-postgres", "check-health", "--port", "8080"],
        )
        .is_ok_and(|output| output.status.success())
    });
    assert!(
        healthy,
        "the connector did not become healthy within {HEALTHY_DEADLINE:?}"
    );
}
