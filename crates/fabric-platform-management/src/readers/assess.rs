//! Holding one sample to the record the last accepted pass stamped.

use fabric_runtime_publication::RecordStamp;

use super::finding::MemberFinding;
use super::reasons::reasons;
use super::sample::{ReaderSample, SampleOutcome};
use crate::{GateStatus, ReadinessReport};

/// The four answers; see the module documentation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReaderState {
    /// Every replica that could serve reports the stamped record.
    Applied,
    /// Some replica is behind, held, not ready or terminating.
    Pending,
    /// Nothing trustworthy to compare.
    Unobserved,
    /// Replicas are deliberately zero.
    NoReaders,
}

/// What the readers were found serving, and why that is the answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReaderApplication {
    /// The answer.
    pub state: ReaderState,
    /// Why, in an operator's words: every reason found, unobserved ones
    /// first.
    pub reasons: Vec<String>,
    /// The record the last accepted pass stamped, if one has.
    pub expected: Option<RecordStamp>,
    /// When the sample finished, if one was taken.
    pub observed_at_unix_seconds: Option<u64>,
    /// The replicas the workload asks for, if it was read.
    pub desired_replicas: Option<u32>,
    /// Every pod the workload owns, judged.
    pub members: Vec<MemberFinding>,
}

/// Holds `sample` to `expected`. `sample` is `None` when no observer is
/// configured, which is unobserved.
#[must_use]
pub fn assess(expected: Option<RecordStamp>, sample: Option<&ReaderSample>) -> ReaderApplication {
    let mut application = ReaderApplication {
        state: ReaderState::Unobserved,
        reasons: Vec::new(),
        expected,
        observed_at_unix_seconds: sample.map(|sample| sample.observed_at_unix_seconds),
        desired_replicas: None,
        members: Vec::new(),
    };
    let Some(sample) = sample else {
        application.reasons.push(
            "no reader observation is configured, so nothing reads what the runtime's replicas serve"
                .to_owned(),
        );
        return application;
    };
    let (desired, rollout_observed, members) = match &sample.outcome {
        SampleOutcome::Unavailable { detail } => {
            application.reasons.push(detail.clone());
            return application;
        }
        SampleOutcome::Observed {
            desired_replicas,
            rollout_observed,
            members,
        } => (*desired_replicas, *rollout_observed, members),
    };
    application.desired_replicas = Some(desired);
    application.members = members
        .iter()
        .map(|member| MemberFinding::of(member, expected))
        .collect();
    if desired == 0 && members.is_empty() {
        application.state = ReaderState::NoReaders;
        application.reasons.push(
            "replicas are 0, so no replica serves: neither applied nor evidence that anything was withdrawn"
                .to_owned(),
        );
        return application;
    }

    let (unobserved, pending) = reasons(expected, desired, rollout_observed, &application.members);
    application.state = if !unobserved.is_empty() {
        ReaderState::Unobserved
    } else if !pending.is_empty() {
        ReaderState::Pending
    } else {
        ReaderState::Applied
    };
    application.reasons = unobserved.into_iter().chain(pending).collect();
    if let (ReaderState::Applied, Some(expected)) = (application.state, expected) {
        application.reasons.push(format!(
            "in this sample every replica that could serve reports generation {} with the stamped record; \
             not a guarantee about later or in-flight requests",
            expected.generation
        ));
    }
    application
}

/// Whether a readiness-gated update is complete: the documents are
/// published and every replica that could serve applied them. Never at
/// replicas 0, where there is no reader evidence to require.
#[must_use]
pub fn update_complete(readiness: &ReadinessReport, readers: &ReaderApplication) -> bool {
    readiness.documents == GateStatus::Met && readers.state == ReaderState::Applied
}
