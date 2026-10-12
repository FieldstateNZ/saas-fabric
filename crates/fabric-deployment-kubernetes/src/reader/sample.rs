//! One sample: read the workload, ask every ready pod it owns, read the
//! workload again, and keep the answers only if nothing moved in between.

use std::net::IpAddr;
use std::sync::Arc;

use fabric_platform_management::{MemberState, ProbeFailure, ProbeOutcome, ReaderMember, SampleOutcome};
use tokio::task::JoinSet;

use super::cluster::Cluster;
use super::membership::{owned_pods, owned_sets, Membership};
use super::probe::Probe;
use super::ReaderTarget;
use crate::client::selector_of;
use crate::wire::Pod;

/// More owned pods than this is not a runtime this sample is bounded for.
const MAX_MEMBERS: usize = 64;

pub(super) async fn sample(
    cluster: &dyn Cluster,
    probe: &Arc<dyn Probe>,
    target: &ReaderTarget,
) -> Result<SampleOutcome, String> {
    let (namespace, name) = (target.namespace.as_str(), target.deployment.as_str());
    let before = cluster.deployment(namespace, name).await?;
    let selector = selector_of(&before)?;
    let sets = cluster.replica_sets(namespace, &selector).await?;
    let pods = cluster.pods(namespace, &selector).await?;
    let owned = owned_pods(&owned_sets(&before, &sets), &pods);
    if owned.len() > MAX_MEMBERS {
        return Err(format!(
            "The runtime owns more than {MAX_MEMBERS} pods, more than one sample asks."
        ));
    }
    let members = ask_each(&owned, probe).await;

    let after = cluster.deployment(namespace, name).await?;
    let after_sets = cluster.replica_sets(namespace, &selector).await?;
    let after_pods = cluster.pods(namespace, &selector).await?;
    if Membership::of(&before, &sets, &pods) != Membership::of(&after, &after_sets, &after_pods) {
        return Err("The runtime's replicas changed during the sample; refresh to read them again.".into());
    }
    Ok(SampleOutcome::Observed {
        desired_replicas: after.spec.replicas,
        rollout_observed: after.status.observed_generation >= after.metadata.generation
            && after.metadata.deletion_timestamp.is_none(),
        members,
    })
}

/// Every owned pod's state, asking each ready one at once.
async fn ask_each(owned: &[(&Pod, String)], probe: &Arc<dyn Probe>) -> Vec<ReaderMember> {
    let mut states: Vec<Option<MemberState>> = owned.iter().map(|(pod, _)| unasked(pod)).collect();
    let mut asking = JoinSet::new();
    for (index, (pod, _)) in owned.iter().enumerate() {
        if states.get(index).is_some_and(Option::is_none) {
            let address = pod
                .status
                .pod_ip
                .as_deref()
                .and_then(|ip| ip.parse::<IpAddr>().ok());
            let probe = Arc::clone(probe);
            asking.spawn(async move {
                let outcome = match address {
                    Some(address) => probe.ask(address).await,
                    None => ProbeOutcome::Failed(ProbeFailure::NoAddress),
                };
                (index, outcome)
            });
        }
    }
    while let Some(asked) = asking.join_next().await {
        if let Ok((index, outcome)) = asked {
            if let Some(slot) = states.get_mut(index) {
                *slot = Some(MemberState::Probed(outcome));
            }
        }
    }
    owned
        .iter()
        .zip(states)
        .map(|((pod, replica_set), state)| ReaderMember {
            pod: pod.metadata.name.clone(),
            replica_set: replica_set.clone(),
            state: state.unwrap_or(MemberState::Probed(ProbeOutcome::Failed(
                ProbeFailure::Unreachable,
            ))),
        })
        .collect()
}

/// A pod's state when it is not to be asked: terminating, or not ready.
fn unasked(pod: &Pod) -> Option<MemberState> {
    if pod.metadata.deletion_timestamp.is_some() {
        return Some(MemberState::Terminating);
    }
    let ready = pod
        .status
        .conditions
        .iter()
        .any(|condition| condition.r#type == "Ready" && condition.status == "True");
    (!ready).then_some(MemberState::NotReady)
}
