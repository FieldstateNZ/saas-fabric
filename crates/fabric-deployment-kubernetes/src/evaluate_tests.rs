#![allow(clippy::unwrap_used, clippy::indexing_slicing)]
use super::*;
use serde_json::json;
fn fixture() -> (WorkloadTarget, Deployment, Vec<ReplicaSet>, Vec<Pod>) {
    let image = format!("example/fabric:v1.2.3@sha256:{}", "a".repeat(64));
    let target = WorkloadTarget {
        namespace: "operator-system".into(),
        deployment: "api".into(),
        container: "api".into(),
        repository: "example/fabric".into(),
    };
    let deployment = serde_json::from_value(json!({"metadata":{"uid":"dep","resourceVersion":"1","generation":2},"spec":{"replicas":1,"selector":{"matchLabels":{"app":"api"}},"template":{"spec":{"containers":[{"name":"api","image":image}]}}},"status":{"observedGeneration":2,"replicas":1,"updatedReplicas":1,"readyReplicas":1,"availableReplicas":1}})).unwrap();
    let sets = serde_json::from_value(json!([{"metadata":{"uid":"set","resourceVersion":"1","ownerReferences":[{"uid":"dep","kind":"Deployment","controller":true}]}}])).unwrap();
    let pods = serde_json::from_value(json!([{"metadata":{"uid":"pod","resourceVersion":"1","ownerReferences":[{"uid":"set","kind":"ReplicaSet","controller":true}]},"spec":{"containers":[{"name":"api","image":image}]},"status":{"containerStatuses":[{"name":"api","ready":true,"imageID":format!("example/fabric@sha256:{}","a".repeat(64)),"state":{"running":{}}}],"conditions":[{"type":"Ready","status":"True"}]}}])).unwrap();
    (target, deployment, sets, pods)
}
#[test]
fn requires_controller_and_actual_container_evidence() {
    let (target, mut dep, sets, mut pods) = fixture();
    assert_eq!(evaluate(&target, &dep, &sets, &pods).health, Health::Healthy);
    dep.status.observed_generation = 1;
    assert_eq!(evaluate(&target, &dep, &sets, &pods).health, Health::Progressing);
    dep.status.observed_generation = 2;
    pods[0].status.container_statuses[0].image_id = "sha256:wrong".into();
    let result = evaluate(&target, &dep, &sets, &pods);
    assert_eq!(result.health, Health::Progressing);
    assert!(result.versions.is_empty());
}
#[test]
fn foreign_pods_cannot_satisfy_rollout() {
    let (target, dep, mut sets, pods) = fixture();
    sets[0].metadata.owner_references[0].uid = "other".into();
    assert_eq!(evaluate(&target, &dep, &sets, &pods).ready_replicas, 0);
    assert_eq!(evaluate(&target, &dep, &sets, &pods).health, Health::Progressing);
}
#[test]
fn terminating_and_unready_pods_are_not_healthy() {
    let (target, dep, sets, mut pods) = fixture();
    pods[0].metadata.deletion_timestamp = Some("now".into());
    assert_eq!(evaluate(&target, &dep, &sets, &pods).health, Health::Progressing);
    pods[0].metadata.deletion_timestamp = None;
    pods[0].status.conditions.clear();
    assert_eq!(evaluate(&target, &dep, &sets, &pods).health, Health::Progressing);
}
#[test]
fn stopped_workloads_require_completed_scale_down() {
    let (target, mut dep, sets, pods) = fixture();
    dep.spec.replicas = 0;
    assert_eq!(evaluate(&target, &dep, &sets, &pods).health, Health::Progressing);
    dep.status.replicas = 0;
    assert_eq!(evaluate(&target, &dep, &sets, &[]).health, Health::Stopped);
}
#[test]
fn failed_controller_overrides_ready_counts() {
    let (target, mut dep, sets, pods) = fixture();
    dep.status.conditions.push(crate::wire::Condition {
        r#type: "Progressing".into(),
        status: "False".into(),
    });
    assert_eq!(evaluate(&target, &dep, &sets, &pods).health, Health::Degraded);
}
#[test]
fn tag_only_and_wrong_repository_images_are_not_version_evidence() {
    let (target, mut dep, sets, mut pods) = fixture();
    dep.spec.template.spec.containers[0].image = "example/fabric:v1.2.3".into();
    assert_eq!(evaluate(&target, &dep, &sets, &pods).health, Health::Unavailable);
    pods[0].spec.containers[0].image = pods[0].spec.containers[0]
        .image
        .replace("example/fabric", "example/foreign");
    assert!(evaluate(&target, &dep, &sets, &pods).versions.is_empty());
}
