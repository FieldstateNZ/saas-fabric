//! Preserve disagreements and stopped workloads rather than inventing a release.
use fabric_platform_management::{DeploymentHealth as Health, DeploymentObservation, WorkloadObservation};
use std::collections::BTreeSet;

pub(crate) fn summarize(
    workloads: Vec<WorkloadObservation>,
    at: u64,
    detail: Option<String>,
) -> DeploymentObservation {
    let health = if workloads.is_empty() || workloads.iter().any(|w| w.health == Health::Unavailable) {
        Health::Unavailable
    } else if workloads.iter().any(|w| w.health == Health::Degraded) {
        Health::Degraded
    } else if workloads.iter().any(|w| w.health == Health::Progressing) {
        Health::Progressing
    } else if workloads.iter().all(|w| w.health == Health::Stopped) {
        Health::Stopped
    } else {
        Health::Healthy
    };
    let versions: BTreeSet<_> = workloads
        .iter()
        .filter(|w| w.health != Health::Stopped)
        .flat_map(|w| w.versions.iter().cloned())
        .collect();
    let version = if health == Health::Healthy && versions.len() == 1 {
        versions.into_iter().next()
    } else {
        None
    };
    let health = if health == Health::Healthy && version.is_none() {
        Health::Progressing
    } else {
        health
    };
    DeploymentObservation {
        observed_at_unix_seconds: at,
        version,
        health,
        workloads,
        detail,
    }
}

#[cfg(test)]
#[path = "summary_tests.rs"]
mod tests;
