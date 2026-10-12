//! A sample reads every pod the runtime Deployment owns, through every
//! replica set, asks only the ready ones, and is discarded if anything moved
//! while it was taken (G7).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeMap;
use std::net::IpAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use fabric_core::{ReaderGenerationReport, RecordDigest, ServedOrigin, SystemClock};
use fabric_platform_management::{
    assess_readers, MemberState, ProbeFailure, ProbeOutcome, ReaderObserver as _, ReaderState, RecordStamp,
    SampleOutcome,
};
use serde_json::{json, Value};

use super::cluster::Cluster;
use super::probe::Probe;
use super::{KubernetesReaderObserver, ReaderTarget};
use crate::wire::{Deployment, Pod, ReplicaSet};

const DEPLOYMENT: &str = "deployment-uid";

fn deployment(resource_version: &str, replicas: u32, observed: u64) -> Value {
    json!({
        "metadata": {"name": "fabric-api", "uid": DEPLOYMENT, "resourceVersion": resource_version, "generation": 3},
        "spec": {"replicas": replicas, "selector": {"matchLabels": {"app": "fabric-api"}},
                 "template": {"spec": {"containers": [{"name": "api", "image": "fabric"}]}}},
        "status": {"observedGeneration": observed}
    })
}

fn replica_set(name: &str, owner: &str) -> Value {
    json!({"metadata": {"name": name, "uid": format!("{name}-uid"), "resourceVersion": "1",
           "ownerReferences": [{"uid": owner, "kind": "Deployment", "controller": true}]}})
}

/// A pod `set` owns, at `ip`. `ready` and `deleting` set its conditions.
fn pod(name: &str, set: &str, ip: Option<&str>, ready: bool, deleting: bool) -> Value {
    let mut pod = json!({
        "metadata": {"name": name, "uid": format!("{name}-uid"), "resourceVersion": "1",
                     "ownerReferences": [{"uid": format!("{set}-uid"), "kind": "ReplicaSet", "controller": true}]},
        "spec": {"containers": [{"name": "api", "image": "fabric"}]},
        "status": {"conditions": [{"type": "Ready", "status": if ready { "True" } else { "False" }}]}
    });
    if let Some(ip) = ip {
        pod["status"]["podIP"] = json!(ip);
    }
    if deleting {
        pod["metadata"]["deletionTimestamp"] = json!("2026-10-12T00:00:00Z");
    }
    pod
}

/// What the API server answers on the first read of each kind, then on
/// every later one.
struct Scripted {
    reads: [Mutex<Vec<Value>>; 3],
    calls: [AtomicUsize; 3],
}

impl Scripted {
    fn steady(deployment: Value, sets: Vec<Value>, pods: Vec<Value>) -> Self {
        Self::moving(
            (deployment.clone(), sets.clone(), pods.clone()),
            (deployment, sets, pods),
        )
    }

    fn moving(before: (Value, Vec<Value>, Vec<Value>), after: (Value, Vec<Value>, Vec<Value>)) -> Self {
        Self {
            reads: [
                Mutex::new(vec![before.0, after.0]),
                Mutex::new(vec![Value::Array(before.1), Value::Array(after.1)]),
                Mutex::new(vec![Value::Array(before.2), Value::Array(after.2)]),
            ],
            calls: Default::default(),
        }
    }

    fn read<T: serde::de::DeserializeOwned>(&self, kind: usize) -> T {
        let call = self.calls[kind].fetch_add(1, Ordering::SeqCst).min(1);
        serde_json::from_value(self.reads[kind].lock().unwrap()[call].clone()).unwrap()
    }
}

#[async_trait]
impl Cluster for Scripted {
    async fn deployment(&self, namespace: &str, name: &str) -> Result<Deployment, String> {
        assert_eq!((namespace, name), ("runtime", "fabric-api"));
        Ok(self.read(0))
    }
    async fn replica_sets(&self, _: &str, selector: &str) -> Result<Vec<ReplicaSet>, String> {
        assert_eq!(selector, "app=fabric-api");
        Ok(self.read(1))
    }
    async fn pods(&self, _: &str, selector: &str) -> Result<Vec<Pod>, String> {
        assert_eq!(selector, "app=fabric-api");
        Ok(self.read(2))
    }
}

/// Answers by address, and records which addresses were asked.
#[derive(Default)]
struct Answers {
    by_address: BTreeMap<IpAddr, ProbeOutcome>,
    asked: Mutex<Vec<IpAddr>>,
}

#[async_trait]
impl Probe for Answers {
    async fn ask(&self, address: IpAddr) -> ProbeOutcome {
        self.asked.lock().unwrap().push(address);
        self.by_address
            .get(&address)
            .cloned()
            .unwrap_or(ProbeOutcome::Failed(ProbeFailure::Unreachable))
    }
}

fn reported(generation: u64, sha: u8) -> ProbeOutcome {
    ProbeOutcome::Reported(ReaderGenerationReport::new(
        generation,
        RecordDigest::from_bytes([sha; 32]),
        ServedOrigin::Published,
    ))
}

fn stamp() -> RecordStamp {
    RecordStamp {
        generation: 5,
        sha256: RecordDigest::from_bytes([5; 32]),
    }
}

fn answers(pairs: &[(&str, ProbeOutcome)]) -> Arc<Answers> {
    Arc::new(Answers {
        by_address: pairs
            .iter()
            .map(|(ip, outcome)| (ip.parse().unwrap(), outcome.clone()))
            .collect(),
        asked: Mutex::default(),
    })
}

fn observer(cluster: Scripted, probe: &Arc<Answers>) -> KubernetesReaderObserver {
    KubernetesReaderObserver {
        cluster: Arc::new(cluster),
        probe: Arc::clone(probe) as Arc<dyn Probe>,
        target: ReaderTarget {
            namespace: "runtime".into(),
            deployment: "fabric-api".into(),
            port: 9091,
        },
        clock: SystemClock::shared(),
    }
}

fn members(outcome: &SampleOutcome) -> Vec<(String, String, MemberState)> {
    let SampleOutcome::Observed { members, .. } = outcome else {
        panic!("expected an observed sample, got {outcome:?}");
    };
    members
        .iter()
        .map(|member| {
            (
                member.pod.clone(),
                member.replica_set.clone(),
                member.state.clone(),
            )
        })
        .collect()
}

#[tokio::test]
async fn every_ready_pod_of_the_current_replica_set_is_asked_and_matches() {
    let probe = answers(&[("10.0.0.1", reported(5, 5)), ("10.0.0.2", reported(5, 5))]);
    let cluster = Scripted::steady(
        deployment("7", 2, 3),
        vec![replica_set("rs-2", DEPLOYMENT)],
        vec![
            pod("api-a", "rs-2", Some("10.0.0.1"), true, false),
            pod("api-b", "rs-2", Some("10.0.0.2"), true, false),
        ],
    );
    let sample = observer(cluster, &probe).sample().await;
    assert_eq!(
        members(&sample.outcome),
        vec![
            ("api-a".into(), "rs-2".into(), MemberState::Probed(reported(5, 5))),
            ("api-b".into(), "rs-2".into(), MemberState::Probed(reported(5, 5))),
        ]
    );
    assert_eq!(
        assess_readers(Some(stamp()), Some(&sample)).state,
        ReaderState::Applied
    );
}

#[tokio::test]
async fn a_ready_pod_of_an_older_replica_set_is_asked_too_and_holds_the_sample_back() {
    let probe = answers(&[
        ("10.0.0.1", reported(5, 5)),
        ("10.0.0.2", reported(5, 5)),
        ("10.0.0.9", reported(4, 4)),
    ]);
    let cluster = Scripted::steady(
        deployment("7", 2, 3),
        vec![replica_set("rs-2", DEPLOYMENT), replica_set("rs-1", DEPLOYMENT)],
        vec![
            pod("api-a", "rs-2", Some("10.0.0.1"), true, false),
            pod("api-b", "rs-2", Some("10.0.0.2"), true, false),
            pod("api-old", "rs-1", Some("10.0.0.9"), true, false),
        ],
    );
    let sample = observer(cluster, &probe).sample().await;
    assert!(members(&sample.outcome).contains(&(
        "api-old".into(),
        "rs-1".into(),
        MemberState::Probed(reported(4, 4))
    )));
    assert_eq!(
        assess_readers(Some(stamp()), Some(&sample)).state,
        ReaderState::Pending
    );
}

#[tokio::test]
async fn pods_and_replica_sets_the_deployment_does_not_control_are_left_out() {
    let probe = answers(&[("10.0.0.1", reported(5, 5)), ("10.0.0.7", reported(9, 9))]);
    let mut foreign_pod = pod("stranger", "rs-2", Some("10.0.0.7"), true, false);
    foreign_pod["metadata"]["ownerReferences"][0]["controller"] = json!(false);
    let cluster = Scripted::steady(
        deployment("7", 1, 3),
        vec![
            replica_set("rs-2", DEPLOYMENT),
            replica_set("rs-other", "another-deployment"),
        ],
        vec![
            pod("api-a", "rs-2", Some("10.0.0.1"), true, false),
            pod("other", "rs-other", Some("10.0.0.7"), true, false),
            foreign_pod,
        ],
    );
    let sample = observer(cluster, &probe).sample().await;
    assert_eq!(members(&sample.outcome).len(), 1);
    assert_eq!(
        *probe.asked.lock().unwrap(),
        vec!["10.0.0.1".parse::<IpAddr>().unwrap()]
    );
}

#[tokio::test]
async fn terminating_and_unready_pods_are_not_asked_and_a_pod_with_no_address_is_not_guessed_at() {
    let probe = answers(&[("10.0.0.1", reported(5, 5))]);
    let cluster = Scripted::steady(
        deployment("7", 2, 3),
        vec![replica_set("rs-2", DEPLOYMENT)],
        vec![
            pod("api-a", "rs-2", Some("10.0.0.1"), true, false),
            pod("api-gone", "rs-2", Some("10.0.0.2"), true, true),
            pod("api-new", "rs-2", Some("10.0.0.3"), false, false),
            pod("api-bare", "rs-2", None, true, false),
            pod("api-odd", "rs-2", Some("not-an-address"), true, false),
        ],
    );
    let sample = observer(cluster, &probe).sample().await;
    let states: Vec<MemberState> = members(&sample.outcome)
        .into_iter()
        .map(|member| member.2)
        .collect();
    assert_eq!(
        states,
        vec![
            MemberState::Probed(reported(5, 5)),
            MemberState::Terminating,
            MemberState::NotReady,
            MemberState::Probed(ProbeOutcome::Failed(ProbeFailure::NoAddress)),
            MemberState::Probed(ProbeOutcome::Failed(ProbeFailure::NoAddress)),
        ]
    );
    assert_eq!(
        *probe.asked.lock().unwrap(),
        vec!["10.0.0.1".parse::<IpAddr>().unwrap()]
    );
}

#[tokio::test]
async fn anything_that_moves_during_the_sample_discards_it() {
    let sets = || vec![replica_set("rs-2", DEPLOYMENT)];
    let ready = |rv: &str, uid: &str| {
        let mut pod = pod("api-a", "rs-2", Some("10.0.0.1"), true, false);
        pod["metadata"]["resourceVersion"] = json!(rv);
        pod["metadata"]["uid"] = json!(uid);
        pod
    };
    let before = (deployment("7", 1, 3), sets(), vec![ready("1", "a")]);
    for after in [
        (deployment("8", 1, 3), sets(), vec![ready("1", "a")]),
        (deployment("7", 1, 3), sets(), vec![ready("2", "a")]),
        (deployment("7", 1, 3), sets(), vec![ready("1", "replaced")]),
        (
            deployment("7", 1, 3),
            vec![replica_set("rs-2", DEPLOYMENT), replica_set("rs-3", DEPLOYMENT)],
            vec![ready("1", "a")],
        ),
        (
            deployment("7", 1, 3),
            sets(),
            vec![
                ready("1", "a"),
                pod("api-b", "rs-2", Some("10.0.0.2"), false, false),
            ],
        ),
    ] {
        let probe = answers(&[("10.0.0.1", reported(5, 5))]);
        let sample = observer(Scripted::moving(before.clone(), after), &probe)
            .sample()
            .await;
        let SampleOutcome::Unavailable { detail } = &sample.outcome else {
            panic!("a moving sample was kept: {:?}", sample.outcome);
        };
        assert!(detail.contains("changed during the sample"), "{detail}");
        assert_eq!(
            assess_readers(Some(stamp()), Some(&sample)).state,
            ReaderState::Unobserved
        );
    }
}

#[tokio::test]
async fn a_rollout_the_controller_has_not_observed_is_reported_as_such() {
    let probe = answers(&[("10.0.0.1", reported(5, 5))]);
    let cluster = Scripted::steady(
        deployment("7", 1, 2),
        vec![replica_set("rs-2", DEPLOYMENT)],
        vec![pod("api-a", "rs-2", Some("10.0.0.1"), true, false)],
    );
    let sample = observer(cluster, &probe).sample().await;
    let SampleOutcome::Observed { rollout_observed, .. } = sample.outcome else {
        panic!("expected an observed sample");
    };
    assert!(!rollout_observed);
}

#[tokio::test]
async fn zero_replicas_and_no_pods_is_no_readers() {
    let probe = answers(&[]);
    let cluster = Scripted::steady(
        deployment("7", 0, 3),
        vec![replica_set("rs-2", DEPLOYMENT)],
        vec![],
    );
    let sample = observer(cluster, &probe).sample().await;
    assert_eq!(
        assess_readers(Some(stamp()), Some(&sample)).state,
        ReaderState::NoReaders
    );
}

#[tokio::test]
async fn more_pods_than_one_sample_asks_is_refused_without_asking_any() {
    let probe = answers(&[]);
    let pods = (0..65)
        .map(|index| pod(&format!("api-{index}"), "rs-2", Some("10.0.0.1"), true, false))
        .collect();
    let cluster = Scripted::steady(
        deployment("7", 65, 3),
        vec![replica_set("rs-2", DEPLOYMENT)],
        pods,
    );
    let sample = observer(cluster, &probe).sample().await;
    assert!(matches!(sample.outcome, SampleOutcome::Unavailable { .. }));
    assert!(probe.asked.lock().unwrap().is_empty());
}

#[test]
fn a_target_must_name_its_deployment_and_its_port() {
    let target = |namespace: &str, deployment: &str, port: u16| ReaderTarget {
        namespace: namespace.into(),
        deployment: deployment.into(),
        port,
    };
    assert!(target("runtime", "fabric-api", 9091).validate().is_ok());
    assert!(target("runtime", "fabric-api", 0).validate().is_err());
    assert!(target("Runtime", "fabric-api", 9091).validate().is_err());
    assert!(target("runtime", "", 9091).validate().is_err());
    assert!(target("runtime", "../secrets", 9091).validate().is_err());
}
