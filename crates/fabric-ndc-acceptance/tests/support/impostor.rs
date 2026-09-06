//! A real HTTP process that is not an NDC connector.
//!
//! `nginx`, answering `200` on every path with plain text, stands in for "a
//! connector endpoint that answers HTTP but is not NDC" -- the case that
//! distinguishes "the database refused this query" from "this is not an NDC
//! connector at all" in `fabric_connector_ndc::client::response_decoding`
//! (`m2-ndc/plan.md` §3.6). The composed acceptance test points a connector
//! configuration at [`Impostor::start`]'s URL and asserts the refusal.
//!
//! # Why a custom config, not the stock welcome page
//!
//! nginx's shipped `default.conf` only serves `index.html` at `/`; any other
//! path -- including `/capabilities`, the first thing
//! `fabric_connector_ndc::build_ndc_connector` requests -- gets a `404` from
//! the vanilla image. A `404` is [`fabric_connector::ConnectorError::Rejected`],
//! the "the backend told us it failed" branch, not the `MalformedResponse`
//! branch this fixture exists to exercise: a *success* status whose body is
//! not NDC. So `Impostor::start` mounts one `location / { return 200 ...; }`
//! block over `/etc/nginx/conf.d/default.conf`, answering `200` for
//! `/capabilities` exactly as it does for `/`.

use std::path::PathBuf;
use std::time::Duration;

use crate::support::docker;
use crate::support::images;
use crate::support::names::RunId;

/// How long to wait for nginx to accept connections before giving up.
const READY_DEADLINE: Duration = Duration::from_secs(15);

/// Answers `200` with plain text on every path, `/capabilities` included --
/// the shape that exercises `MalformedResponse` rather than `Rejected`. See
/// this module's doc comment for why the stock config will not do.
const IMPOSTOR_CONFIG: &str = "\
server {
    listen 80;
    location / {
        default_type text/plain;
        return 200 'not an NDC connector';
    }
}
";

/// A running nginx container on its own network, reached over an ephemeral
/// host port.
///
/// Self-contained rather than joining a [`crate::support::stack::Stack`]'s
/// network: nginx is reached over its published host port exactly the way
/// the real connector is (`http://127.0.0.1:<port>`), never over Docker's
/// internal network DNS, so it has no need to share a network with postgres
/// or the connector at all. Dropping this removes the container, its
/// mounted config directory, and the network; there is no separate teardown
/// to call.
pub struct Impostor {
    /// Base URL of nginx's ephemeral host port.
    pub base_url: String,
    container: docker::Container,
    config_dir: PathBuf,
    network: String,
}

impl Impostor {
    /// Sweeps stale resources left by a prior hard-killed run, then starts
    /// nginx on a fresh network of its own and waits for it to answer.
    #[must_use]
    pub fn start() -> Self {
        crate::support::names::sweep_stale();

        let run_id = RunId::new();
        let network = run_id.network_name();
        docker::network_create(&network)
            .unwrap_or_else(|error| panic!("could not create {network}: {error}"));

        let config_dir = write_config_dir(&run_id);

        let container = docker::run(&docker::RunSpec {
            name: run_id.container_name("nginx"),
            image: images::NGINX.to_owned(),
            network: network.clone(),
            env: Vec::new(),
            publish: Some(80),
            mount_ro: Some((config_dir.clone(), "/etc/nginx/conf.d".to_owned())),
            command: Vec::new(),
        })
        .unwrap_or_else(|error| panic!("could not start nginx: {error}"));

        let host_port = docker::port(&container, 80)
            .unwrap_or_else(|error| panic!("could not read nginx's published port: {error}"));

        wait_ready(&container);

        Self {
            base_url: format!("http://127.0.0.1:{host_port}"),
            container,
            config_dir,
            network,
        }
    }
}

impl Drop for Impostor {
    fn drop(&mut self) {
        let _ = docker::rm(&self.container);
        let _ = std::fs::remove_dir_all(&self.config_dir);
        let _ = docker::network_rm(&self.network);
    }
}

/// Writes [`IMPOSTOR_CONFIG`] as `default.conf` into a fresh directory --
/// the filename nginx's image loads from `/etc/nginx/conf.d` by default --
/// and returns that directory.
fn write_config_dir(run_id: &RunId) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("{}-config", run_id.container_name("nginx")));
    std::fs::create_dir_all(&dir)
        .unwrap_or_else(|error| panic!("could not create {}: {error}", dir.display()));

    std::fs::write(dir.join("default.conf"), IMPOSTOR_CONFIG)
        .unwrap_or_else(|error| panic!("could not write default.conf into {}: {error}", dir.display()));

    dir
}

/// Unlike the distroless connector image, this one has a shell and `wget`
/// (busybox, built into the alpine base) -- enough to prove nginx is
/// actually answering `/capabilities` with `200`, not merely running.
fn wait_ready(container: &docker::Container) {
    let ready = docker::poll_until(READY_DEADLINE, || {
        docker::exec(
            container,
            &[
                "wget",
                "-q",
                "-O",
                "/dev/null",
                "http://127.0.0.1:80/capabilities",
            ],
        )
        .is_ok_and(|output| output.status.success())
    });
    assert!(ready, "nginx did not become ready within {READY_DEADLINE:?}");
}
