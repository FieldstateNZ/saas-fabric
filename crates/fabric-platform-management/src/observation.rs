//! Read-only evidence from the system that runs a component.
use async_trait::async_trait;
use serde::Serialize;

/// The health of an observed workload or release.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DeploymentHealth {
    /// Every requested replica runs its pinned image and is ready.
    Healthy,
    /// The controller or its replicas have not completed the rollout.
    Progressing,
    /// The deployment reports a failed rollout or replica failure.
    Degraded,
    /// No replicas are requested and none are still present.
    Stopped,
    /// Evidence could not be obtained or verified.
    Unavailable,
}

/// Evidence about a named workload; contains no provider credentials.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkloadObservation {
    /// An operator-readable workload name.
    pub name: String,
    /// Current rollout health.
    pub health: DeploymentHealth,
    /// Versions verified against running containers' pinned image digests.
    pub versions: Vec<String>,
    /// Requested replica count, when readable.
    pub desired_replicas: Option<u32>,
    /// Ready, owned pods with a verified running container.
    pub ready_replicas: u32,
    /// A controlled explanation, never a provider response body.
    pub detail: Option<String>,
}

/// A point-in-time observation, independent of desired state in Git.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeploymentObservation {
    /// Time this read completed, not the last successful historical read.
    pub observed_at_unix_seconds: u64,
    /// One version only when all active workloads are healthy and agree.
    pub version: Option<String>,
    /// Aggregate rollout health; stopped workloads remain separately visible.
    pub health: DeploymentHealth,
    /// Evidence for every configured workload, including stopped ones.
    pub workloads: Vec<WorkloadObservation>,
    /// Safe diagnostic for an unavailable observation.
    pub detail: Option<String>,
}

/// Observes deployment evidence without creating or changing resources.
#[async_trait]
pub trait DeploymentObserver: Send + Sync {
    /// `None` means this component has no observation binding.
    async fn observe(&self, environment: &str, component: &str) -> Option<DeploymentObservation>;
}
