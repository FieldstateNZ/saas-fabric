//! What one replica's answer means against the stamped record.

use fabric_core::{ReaderGenerationReport, ServedOrigin};
use fabric_runtime_publication::RecordStamp;

use super::sample::{MemberState, ProbeFailure, ProbeOutcome, ReaderMember};

/// One replica, judged.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemberFinding {
    /// The pod's name.
    pub pod: String,
    /// The replica set that owns it.
    pub replica_set: String,
    /// What its answer means.
    pub finding: Finding,
    /// What it reported, when it reported a generation.
    pub report: Option<ReaderGenerationReport>,
}

/// What one replica's state or answer means.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Finding {
    /// The stamped generation and record digest, from the published files.
    Matches,
    /// An older generation, from the published files.
    Behind,
    /// Its last-known-good copy, at or below the stamped generation.
    LastKnownGood,
    /// A generation above the stamped one.
    Ahead,
    /// The stamped generation number with another record digest.
    WrongDigest,
    /// No whole recorded generation.
    NothingServed,
    /// No usable answer.
    Failed(ProbeFailure),
    /// Not ready, so not asked.
    NotReady,
    /// Being deleted, so not asked.
    Terminating,
    /// Answered, but nothing was stamped to compare it with.
    Uncompared,
}

impl MemberFinding {
    /// Judges `member` against `expected`.
    pub(super) fn of(member: &ReaderMember, expected: Option<RecordStamp>) -> Self {
        let (finding, report) = match &member.state {
            MemberState::NotReady => (Finding::NotReady, None),
            MemberState::Terminating => (Finding::Terminating, None),
            MemberState::Probed(ProbeOutcome::NothingServed) => (Finding::NothingServed, None),
            MemberState::Probed(ProbeOutcome::Failed(failure)) => (Finding::Failed(*failure), None),
            MemberState::Probed(ProbeOutcome::Reported(report)) => (
                expected.map_or(Finding::Uncompared, |expected| compare(report, expected)),
                Some(*report),
            ),
        };
        Self {
            pod: member.pod.clone(),
            replica_set: member.replica_set.clone(),
            finding,
            report,
        }
    }
}

fn compare(report: &ReaderGenerationReport, expected: RecordStamp) -> Finding {
    if report.generation > expected.generation {
        Finding::Ahead
    } else if report.generation == expected.generation && report.record_sha256 != expected.sha256 {
        Finding::WrongDigest
    } else if report.origin == ServedOrigin::LastKnownGood {
        Finding::LastKnownGood
    } else if report.generation < expected.generation {
        Finding::Behind
    } else {
        Finding::Matches
    }
}

impl std::fmt::Display for ProbeFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoAddress => formatter.write_str("has no address to ask"),
            Self::Unreachable => formatter.write_str("could not be reached"),
            Self::TimedOut => formatter.write_str("did not answer in time"),
            Self::Redirected => formatter.write_str("answered with a redirect, which is not followed"),
            Self::Status(status) => write!(formatter, "answered with status {status}"),
            Self::TooLarge => formatter.write_str("answered with more than a report's size bound"),
            Self::Malformed => formatter.write_str("answered with something that is not a report"),
        }
    }
}
