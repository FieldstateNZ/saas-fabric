//! Which generation each runtime replica serves (G7): the workload's pods
//! read through the same bounded, read-only API calls as deployment
//! evidence, and each ready pod asked on its reader-generation listener.
//!
//! Two clients, never one. The cluster client presents this pod's service
//! account to the API server; the probe client presents nothing to a pod,
//! follows no redirect, uses no proxy, and reads a bounded body within a
//! bounded time. A pod is asked only at the address the API server reports
//! for a pod the configured Deployment owns, on the configured port: never
//! at an address from client desired state or a request.

mod cluster;
mod membership;
mod probe;
mod sample;

#[cfg(test)]
#[path = "reader/sample_tests.rs"]
mod sample_tests;

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use fabric_core::Clock;
use fabric_platform_management::{ReaderObserver, ReaderSample, SampleOutcome};
use serde::Deserialize;

use crate::client::Client;
use cluster::Cluster;
use probe::{HttpProbe, Probe};

/// The bound on one whole sample: the reads, every probe and the re-reads.
const SAMPLE_BUDGET: Duration = Duration::from_secs(8);

/// The runtime Deployment whose replicas report what they serve, and the
/// port their reader-generation listener binds. Deployment-owned
/// configuration, never accepted from an API caller.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReaderTarget {
    /// The runtime's namespace.
    pub namespace: String,
    /// The runtime Deployment's exact name.
    pub deployment: String,
    /// The port of each replica's reader-generation listener.
    pub port: u16,
}

impl ReaderTarget {
    fn validate(&self) -> Result<(), String> {
        for value in [&self.namespace, &self.deployment] {
            let valid = !value.is_empty()
                && value.len() <= 253
                && value
                    .bytes()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-' || c == b'.');
            if !valid {
                return Err("reader observation requires DNS-style namespace and deployment names".into());
            }
        }
        if self.port == 0 {
            return Err("reader observation requires the reader-generation listener's port".into());
        }
        Ok(())
    }
}

/// Samples the runtime's replicas, read-only.
pub struct KubernetesReaderObserver {
    cluster: Arc<dyn Cluster>,
    probe: Arc<dyn Probe>,
    target: ReaderTarget,
    clock: Arc<dyn Clock>,
}

impl KubernetesReaderObserver {
    /// Builds the observer with this pod's service account for the cluster
    /// reads and an anonymous client for the probes.
    ///
    /// # Errors
    /// Refuses an invalid target or a missing cluster trust root.
    pub fn in_cluster(target: ReaderTarget, clock: Arc<dyn Clock>) -> Result<Self, String> {
        target.validate()?;
        Ok(Self {
            cluster: Arc::new(Client::in_cluster()?),
            probe: Arc::new(HttpProbe::new(target.port)?),
            target,
            clock,
        })
    }
}

#[async_trait]
impl ReaderObserver for KubernetesReaderObserver {
    async fn sample(&self) -> ReaderSample {
        let sampled = tokio::time::timeout(
            SAMPLE_BUDGET,
            sample::sample(self.cluster.as_ref(), &self.probe, &self.target),
        )
        .await;
        let outcome = match sampled {
            Ok(Ok(outcome)) => outcome,
            Ok(Err(detail)) => SampleOutcome::Unavailable { detail },
            Err(_) => SampleOutcome::Unavailable {
                detail: "Reader observation timed out; no current evidence is available.".into(),
            },
        };
        ReaderSample {
            observed_at_unix_seconds: self.clock.now_unix_seconds(),
            outcome,
        }
    }
}
