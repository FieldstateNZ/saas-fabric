//! Which pods a Deployment owns, through every one of its replica sets, and
//! whether that set held still for the whole sample.

use std::collections::{BTreeMap, BTreeSet};

use crate::wire::{Deployment, Pod, ReplicaSet};

/// Everything a sample's trust rests on: the Deployment's identity and
/// version, the replica sets it controls, and each owned pod's identity and
/// version. Equal at the first read and the last, or the sample is unusable.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct Membership {
    deployment: (String, String),
    sets: BTreeSet<String>,
    pods: BTreeSet<(String, String)>,
}

/// The replica sets `deployment` controls, by uid, with their names.
pub(super) fn owned_sets(deployment: &Deployment, sets: &[ReplicaSet]) -> BTreeMap<String, String> {
    sets.iter()
        .filter(|set| {
            set.metadata.owner_references.iter().any(|owner| {
                owner.controller && owner.kind == "Deployment" && owner.uid == deployment.metadata.uid
            })
        })
        .map(|set| (set.metadata.uid.clone(), set.metadata.name.clone()))
        .collect()
}

/// Each pod one of `sets` controls, with the name of that replica set.
pub(super) fn owned_pods<'a>(sets: &BTreeMap<String, String>, pods: &'a [Pod]) -> Vec<(&'a Pod, String)> {
    pods.iter()
        .filter_map(|pod| {
            pod.metadata
                .owner_references
                .iter()
                .find(|owner| owner.controller && owner.kind == "ReplicaSet")
                .and_then(|owner| sets.get(&owner.uid))
                .map(|name| (pod, name.clone()))
        })
        .collect()
}

impl Membership {
    pub(super) fn of(deployment: &Deployment, sets: &[ReplicaSet], pods: &[Pod]) -> Self {
        let owned = owned_sets(deployment, sets);
        Self {
            deployment: (
                deployment.metadata.uid.clone(),
                deployment.metadata.resource_version.clone(),
            ),
            pods: owned_pods(&owned, pods)
                .into_iter()
                .map(|(pod, _)| (pod.metadata.uid.clone(), pod.metadata.resource_version.clone()))
                .collect(),
            sets: owned.into_keys().collect(),
        }
    }
}
