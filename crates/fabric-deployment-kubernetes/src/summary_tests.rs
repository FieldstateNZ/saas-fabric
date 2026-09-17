use super::*;
fn row(health: Health, version: &str) -> WorkloadObservation {
    WorkloadObservation {
        name: "api".into(),
        health,
        versions: vec![version.into()],
        desired_replicas: Some(1),
        ready_replicas: 1,
        detail: None,
    }
}
#[test]
fn mixed_releases_never_become_a_single_running_version() {
    let result = summarize(
        vec![row(Health::Healthy, "v1.0.0"), row(Health::Healthy, "v1.0.1")],
        42,
        None,
    );
    assert_eq!(result.health, Health::Progressing);
    assert_eq!(result.version, None);
}
#[test]
fn stopped_runtime_does_not_hide_a_healthy_control_plane() {
    let result = summarize(
        vec![row(Health::Healthy, "v1.0.1"), row(Health::Stopped, "v1.0.0")],
        42,
        None,
    );
    assert_eq!(result.health, Health::Healthy);
    assert_eq!(result.version.as_deref(), Some("v1.0.1"));
}
#[test]
fn missing_or_failed_evidence_cannot_reuse_a_version() {
    for health in [
        Health::Unavailable,
        Health::Degraded,
        Health::Progressing,
        Health::Stopped,
    ] {
        assert_eq!(summarize(vec![row(health, "v1.0.1")], 42, None).version, None);
    }
    assert_eq!(
        summarize(vec![], 42, Some("timeout".into())).health,
        Health::Unavailable
    );
}
