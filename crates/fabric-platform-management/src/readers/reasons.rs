//! Why a sample is not applied.

use fabric_runtime_publication::RecordStamp;

use super::finding::{Finding, MemberFinding};

/// Every reason the sample is not applied: those that make it unobserved,
/// then those that leave it pending.
pub(super) fn reasons(
    expected: Option<RecordStamp>,
    desired: u32,
    rollout_observed: bool,
    members: &[MemberFinding],
) -> (Vec<String>, Vec<String>) {
    let (mut unobserved, mut pending) = (Vec::new(), Vec::new());
    if expected.is_none() {
        unobserved.push("no publication pass has been accepted since the control plane started".to_owned());
    }
    if desired == 0 {
        unobserved.push(format!("replicas are 0 but {} owned pods remain", members.len()));
    }
    if !rollout_observed {
        unobserved.push("the workload's controller has not observed its current spec".to_owned());
    }
    let asked = members
        .iter()
        .filter(|member| !matches!(member.finding, Finding::NotReady | Finding::Terminating))
        .count();
    let asked = u32::try_from(asked).unwrap_or(u32::MAX);
    if desired > 0 && asked == 0 {
        unobserved.push("no replica is ready to ask".to_owned());
    } else if asked < desired {
        pending.push(format!("{asked} of {desired} replicas are ready"));
    }
    for member in members {
        let line = |what: &str| format!("{} ({}) {what}", member.pod, member.replica_set);
        match member.finding {
            Finding::Matches | Finding::Uncompared => {}
            Finding::Behind => pending.push(line("serves an older generation")),
            Finding::LastKnownGood => pending.push(line("serves its last-known-good copy")),
            Finding::NotReady => pending.push(line("is not ready")),
            Finding::Terminating => pending.push(line("is terminating and may still serve")),
            Finding::Ahead => unobserved.push(line("reports a generation above the stamped record")),
            Finding::WrongDigest => {
                unobserved.push(line("reports the stamped generation with another record"));
            }
            Finding::NothingServed => unobserved.push(line("serves no whole recorded generation")),
            Finding::Failed(failure) => unobserved.push(line(&failure.to_string())),
        }
    }
    (unobserved, pending)
}
