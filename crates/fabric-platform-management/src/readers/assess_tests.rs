//! The four answers, against every way a replica can answer (G7).

use fabric_core::{ReaderGenerationReport, RecordDigest, ServedOrigin};
use fabric_runtime_publication::RecordStamp;

use super::*;
use crate::{GateStatus, ReadinessReport};

fn digest(byte: u8) -> RecordDigest {
    RecordDigest::from_bytes([byte; 32])
}

fn stamp() -> RecordStamp {
    RecordStamp {
        generation: 5,
        sha256: digest(5),
    }
}

fn reported(generation: u64, sha: u8, origin: ServedOrigin) -> MemberState {
    MemberState::Probed(ProbeOutcome::Reported(ReaderGenerationReport::new(
        generation,
        digest(sha),
        origin,
    )))
}

fn serving() -> MemberState {
    reported(5, 5, ServedOrigin::Published)
}

fn member(pod: &str, replica_set: &str, state: MemberState) -> ReaderMember {
    ReaderMember {
        pod: pod.to_owned(),
        replica_set: replica_set.to_owned(),
        state,
    }
}

fn sample(desired_replicas: u32, members: Vec<ReaderMember>) -> ReaderSample {
    ReaderSample {
        observed_at_unix_seconds: 1_000,
        outcome: SampleOutcome::Observed {
            desired_replicas,
            rollout_observed: true,
            members,
        },
    }
}

fn state_of(desired: u32, members: Vec<ReaderMember>) -> ReaderApplication {
    assess(Some(stamp()), Some(&sample(desired, members)))
}

#[test]
fn every_ready_replica_on_the_stamped_record_is_applied() {
    let application = state_of(
        2,
        vec![
            member("api-a", "rs-2", serving()),
            member("api-b", "rs-2", serving()),
        ],
    );
    assert_eq!(
        application.state,
        ReaderState::Applied,
        "{:?}",
        application.reasons
    );
    assert_eq!(application.expected, Some(stamp()));
    assert_eq!(application.observed_at_unix_seconds, Some(1_000));
    assert!(application
        .members
        .iter()
        .all(|member| member.finding == Finding::Matches));
    assert!(application.reasons[0].contains("not a guarantee"));
}

#[test]
fn an_older_replica_set_still_ready_during_a_rollout_is_held_to_the_record_too() {
    let behind = state_of(
        2,
        vec![
            member("api-new-a", "rs-2", serving()),
            member("api-new-b", "rs-2", serving()),
            member("api-old", "rs-1", reported(4, 4, ServedOrigin::Published)),
        ],
    );
    assert_eq!(behind.state, ReaderState::Pending);
    assert!(behind
        .reasons
        .iter()
        .any(|reason| reason.contains("api-old (rs-1)")));

    let caught_up = state_of(
        2,
        vec![
            member("api-new-a", "rs-2", serving()),
            member("api-new-b", "rs-2", serving()),
            member("api-old", "rs-1", serving()),
        ],
    );
    assert_eq!(caught_up.state, ReaderState::Applied);
}

#[test]
fn a_terminating_or_unready_pod_or_too_few_ready_replicas_keep_it_pending() {
    for members in [
        vec![
            member("a", "rs", serving()),
            member("b", "rs", serving()),
            member("c", "rs", MemberState::Terminating),
        ],
        vec![
            member("a", "rs", serving()),
            member("b", "rs", serving()),
            member("c", "rs", MemberState::NotReady),
        ],
        vec![member("a", "rs", serving())],
    ] {
        assert_eq!(state_of(2, members).state, ReaderState::Pending);
    }
    let short = state_of(3, vec![member("a", "rs", serving())]);
    assert!(short
        .reasons
        .iter()
        .any(|reason| reason == "1 of 3 replicas are ready"));
}

#[test]
fn an_older_generation_or_a_last_known_good_copy_is_pending() {
    for state in [
        reported(4, 4, ServedOrigin::Published),
        reported(5, 5, ServedOrigin::LastKnownGood),
        reported(3, 3, ServedOrigin::LastKnownGood),
    ] {
        let application = state_of(1, vec![member("a", "rs", state)]);
        assert_eq!(
            application.state,
            ReaderState::Pending,
            "{:?}",
            application.members
        );
    }
}

#[test]
fn an_answer_that_cannot_be_compared_makes_it_unobserved() {
    for (state, finding) in [
        (reported(6, 6, ServedOrigin::Published), Finding::Ahead),
        (reported(6, 6, ServedOrigin::LastKnownGood), Finding::Ahead),
        (reported(5, 9, ServedOrigin::Published), Finding::WrongDigest),
        (reported(5, 9, ServedOrigin::LastKnownGood), Finding::WrongDigest),
        (
            MemberState::Probed(ProbeOutcome::NothingServed),
            Finding::NothingServed,
        ),
        (
            MemberState::Probed(ProbeOutcome::Failed(ProbeFailure::Unreachable)),
            Finding::Failed(ProbeFailure::Unreachable),
        ),
        (
            MemberState::Probed(ProbeOutcome::Failed(ProbeFailure::Malformed)),
            Finding::Failed(ProbeFailure::Malformed),
        ),
        (
            MemberState::Probed(ProbeOutcome::Failed(ProbeFailure::TimedOut)),
            Finding::Failed(ProbeFailure::TimedOut),
        ),
    ] {
        let application = state_of(2, vec![member("a", "rs", serving()), member("b", "rs", state)]);
        assert_eq!(application.state, ReaderState::Unobserved);
        assert_eq!(application.members[1].finding, finding);
    }
}

#[test]
fn unobserved_outranks_pending_and_every_reason_is_kept() {
    let application = state_of(
        2,
        vec![
            member("a", "rs", reported(4, 4, ServedOrigin::Published)),
            member("b", "rs", reported(5, 9, ServedOrigin::Published)),
        ],
    );
    assert_eq!(application.state, ReaderState::Unobserved);
    assert!(application.reasons[0].contains("b (rs)"));
    assert!(application.reasons[1].contains("a (rs)"));
}

#[test]
fn zero_replicas_is_no_readers_and_never_applied() {
    let none = state_of(0, Vec::new());
    assert_eq!(none.state, ReaderState::NoReaders);
    assert!(none.reasons[0].contains("withdrawn"));

    let draining = state_of(0, vec![member("a", "rs", MemberState::Terminating)]);
    assert_eq!(draining.state, ReaderState::Unobserved);
    let lingering = state_of(0, vec![member("a", "rs", serving())]);
    assert_eq!(lingering.state, ReaderState::Unobserved);
}

#[test]
fn desired_replicas_with_none_ready_is_never_a_vacuous_match() {
    assert_eq!(state_of(2, Vec::new()).state, ReaderState::Unobserved);
    let unready = state_of(2, vec![member("a", "rs", MemberState::NotReady)]);
    assert_eq!(unready.state, ReaderState::Unobserved);
    assert!(unready
        .reasons
        .iter()
        .any(|reason| reason == "no replica is ready to ask"));
}

#[test]
fn an_untrustworthy_sample_no_observer_no_stamp_or_an_unobserved_rollout_is_unobserved() {
    let changed = ReaderSample {
        observed_at_unix_seconds: 7,
        outcome: SampleOutcome::Unavailable {
            detail: "the workload's pods changed during the sample".to_owned(),
        },
    };
    let application = assess(Some(stamp()), Some(&changed));
    assert_eq!(application.state, ReaderState::Unobserved);
    assert_eq!(
        application.reasons,
        vec!["the workload's pods changed during the sample"]
    );
    assert_eq!(application.observed_at_unix_seconds, Some(7));

    let unconfigured = assess(Some(stamp()), None);
    assert_eq!(unconfigured.state, ReaderState::Unobserved);
    assert!(unconfigured.reasons[0].contains("no reader observation is configured"));
    assert_eq!(unconfigured.observed_at_unix_seconds, None);

    let unstamped = assess(None, Some(&sample(1, vec![member("a", "rs", serving())])));
    assert_eq!(unstamped.state, ReaderState::Unobserved);
    assert_eq!(unstamped.members[0].finding, Finding::Uncompared);

    let mut rolling = sample(1, vec![member("a", "rs", serving())]);
    if let SampleOutcome::Observed { rollout_observed, .. } = &mut rolling.outcome {
        *rollout_observed = false;
    }
    assert_eq!(
        assess(Some(stamp()), Some(&rolling)).state,
        ReaderState::Unobserved
    );
}

#[test]
fn an_update_is_complete_only_with_the_documents_published_and_the_readers_applied() {
    let met = ReadinessReport {
        documents: GateStatus::Met,
        ..ReadinessReport::of(None)
    };
    let applied = state_of(1, vec![member("a", "rs", serving())]);
    assert!(update_complete(&met, &applied));
    assert!(!update_complete(&ReadinessReport::of(None), &applied));
    for unfinished in [
        state_of(0, Vec::new()),
        state_of(
            1,
            vec![member("a", "rs", reported(4, 4, ServedOrigin::Published))],
        ),
        assess(Some(stamp()), None),
    ] {
        assert!(!update_complete(&met, &unfinished), "{:?}", unfinished.state);
    }
}
