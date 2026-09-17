//! A rollout is healthy only when controller and owned container evidence agree.
use crate::{
    image,
    wire::{Deployment, Pod, ReplicaSet},
    WorkloadTarget,
};
use fabric_platform_management::{DeploymentHealth as Health, WorkloadObservation};
use std::collections::BTreeSet;

pub(crate) fn evaluate(
    target: &WorkloadTarget,
    deployment: &Deployment,
    sets: &[ReplicaSet],
    pods: &[Pod],
) -> WorkloadObservation {
    let desired = deployment.spec.replicas;
    let owned: BTreeSet<_> = sets
        .iter()
        .filter(|set| {
            set.metadata.owner_references.iter().any(|owner| {
                owner.controller && owner.kind == "Deployment" && owner.uid == deployment.metadata.uid
            })
        })
        .map(|set| set.metadata.uid.as_str())
        .collect();
    let pods: Vec<_> = pods
        .iter()
        .filter(|pod| {
            pod.metadata.owner_references.iter().any(|owner| {
                owner.controller && owner.kind == "ReplicaSet" && owned.contains(owner.uid.as_str())
            })
        })
        .collect();
    let expected = deployment
        .spec
        .template
        .spec
        .containers
        .iter()
        .find(|c| c.name == target.container)
        .and_then(|c| image::pinned(&c.image, &target.repository));
    let mut versions = BTreeSet::new();
    let mut ready = 0;
    let mut matching = 0;
    for pod in &pods {
        let container = pod.spec.containers.iter().find(|c| c.name == target.container);
        let status = pod
            .status
            .container_statuses
            .iter()
            .find(|c| c.name == target.container);
        if let (Some(container), Some(status)) = (container, status) {
            if let Some((version, digest)) = image::pinned(&container.image, &target.repository) {
                if status.state.contains_key("running") && image::running_digest(&status.image_id, digest) {
                    versions.insert(version.to_owned());
                    let pod_ready = pod
                        .status
                        .conditions
                        .iter()
                        .any(|c| c.r#type == "Ready" && c.status == "True");
                    if status.ready && pod_ready && pod.metadata.deletion_timestamp.is_none() {
                        ready += 1;
                        if expected == Some((version, digest)) {
                            matching += 1;
                        }
                    }
                }
            }
        }
    }
    let status = &deployment.status;
    let failed = status.conditions.iter().any(|c| {
        (c.r#type == "Progressing" && c.status == "False")
            || (c.r#type == "ReplicaFailure" && c.status == "True")
    });
    let observed = status.observed_generation >= deployment.metadata.generation
        && deployment.metadata.deletion_timestamp.is_none();
    let health = if failed {
        Health::Degraded
    } else if desired == 0 && observed && pods.is_empty() && status.replicas == 0 {
        Health::Stopped
    } else if expected.is_none() {
        Health::Unavailable
    } else if observed
        && desired > 0
        && matching == desired
        && pods.len() == desired as usize
        && status.replicas == desired
        && status.updated_replicas == desired
        && status.ready_replicas == desired
        && status.available_replicas == desired
    {
        Health::Healthy
    } else {
        Health::Progressing
    };
    WorkloadObservation {
        name: target.deployment.clone(), health, versions: versions.into_iter().collect(),
        desired_replicas: Some(desired), ready_replicas: ready,
        detail: (health == Health::Unavailable).then(|| "The configured container needs a versioned, digest-pinned image from its expected repository.".into()),
    }
}

#[cfg(test)]
#[path = "evaluate_tests.rs"]
mod tests;
