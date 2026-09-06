//! `Stack::up` assembles network + postgres (seeded) + connector for one
//! test; `Drop` tears it down in reverse.

use std::path::PathBuf;

use crate::support::connector::{self, ConnectorMode};
use crate::support::docker;
use crate::support::names;
use crate::support::postgres;

/// Everything one test needs: the connector's base URL, and a handle to the
/// shared table for "did the seed really land" assertions.
///
/// Dropping this removes the connector, postgres, the connector's
/// configuration directory, and the network -- in that order, since a
/// network cannot be removed while a container still references it.
pub struct Stack {
    /// Base URL of the connector's ephemeral host port.
    pub connector_base_url: String,
    postgres: docker::Container,
    connector: Option<docker::Container>,
    config_dir: PathBuf,
    network: String,
}

impl Stack {
    /// Sweeps stale resources left by a prior hard-killed run, then brings
    /// up a fresh network, seeded postgres, and the connector in `mode`.
    #[must_use]
    pub fn up(mode: ConnectorMode) -> Self {
        names::sweep_stale();

        let run_id = names::RunId::new();
        let network = run_id.network_name();
        docker::network_create(&network)
            .unwrap_or_else(|error| panic!("could not create {network}: {error}"));

        let postgres = postgres::start(&run_id, &network);
        let (connector_container, connector_base_url, config_dir) =
            connector::start(&run_id, &network, &postgres, mode);

        Self {
            connector_base_url,
            postgres,
            connector: Some(connector_container),
            config_dir,
            network,
        }
    }

    /// Stops the connector without removing it -- the "connector went
    /// unreachable mid-run" case a later slice of issue #62 drives a
    /// request against. `Drop` still removes the (now-stopped) container
    /// afterwards; `docker rm -f` on an already-stopped container is not an
    /// error.
    pub fn stop_connector(&mut self) {
        if let Some(connector) = &self.connector {
            docker::stop(connector).unwrap_or_else(|error| panic!("could not stop the connector: {error}"));
        }
    }

    /// Runs `sql` against the shared table directly, bypassing the
    /// connector under test.
    ///
    /// For "the row really exists" assertions: the connector whose
    /// predicate is being tested cannot also be the one that proves the row
    /// it (correctly or incorrectly) omitted was ever there.
    #[must_use]
    pub fn query_scalar(&self, sql: &str) -> String {
        postgres::query_scalar(&self.postgres, sql)
    }
}

impl Drop for Stack {
    fn drop(&mut self) {
        if let Some(connector) = &self.connector {
            let _ = docker::rm(connector);
        }
        let _ = docker::rm(&self.postgres);
        let _ = std::fs::remove_dir_all(&self.config_dir);
        let _ = docker::network_rm(&self.network);
    }
}
