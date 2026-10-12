//! The readers row: which generation the runtime's replicas were found
//! serving, against the record the last accepted pass stamped (G7).

use fabric_core::ServedOrigin;
use fabric_platform_management::{Finding, MemberFinding, ReaderApplication, ReaderState};

use crate::registration::PlatformBinding;

/// Samples the readers, when an observer is configured, and holds the
/// sample to the record the last accepted pass stamped.
pub(crate) async fn observe_readers(platform: &PlatformBinding) -> ReaderApplication {
    let sample = match &platform.readers {
        Some(observer) => Some(observer.sample().await),
        None => None,
    };
    fabric_platform_management::assess_readers(platform.publication.expected_record(), sample.as_ref())
}

/// What the readers were found serving. A timestamped sample of replicas
/// that could serve, not a guarantee about later or in-flight requests.
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadersRow {
    /// `applied`, `pending`, `unobserved` or `noReaders`.
    pub state: &'static str,
    /// Why, in an operator's words.
    pub reasons: Vec<String>,
    /// The record the last accepted pass stamped, if one has.
    pub expected: Option<StampRow>,
    /// When the sample finished, if one was taken.
    pub observed_at_unix_seconds: Option<u64>,
    /// The replicas the runtime asks for, if it was read.
    pub desired_replicas: Option<u32>,
    /// Every pod the runtime owns, through every replica set.
    pub members: Vec<MemberRow>,
}

/// A record, as a reader identifies it.
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StampRow {
    /// Its generation.
    pub generation: u64,
    /// The SHA-256 of its bytes.
    pub record_sha256: String,
}

/// One pod, judged.
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemberRow {
    /// The pod.
    pub pod: String,
    /// The replica set that owns it.
    pub replica_set: String,
    /// What its answer means.
    pub finding: &'static str,
    /// Why it gave no usable answer, when it gave none.
    pub detail: Option<String>,
    /// What it reported serving, when it reported a generation.
    pub served: Option<ServedRow>,
}

/// A generation a pod reported serving.
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServedRow {
    /// Its generation.
    pub generation: u64,
    /// The SHA-256 of its record.
    pub record_sha256: String,
    /// `published` or `lastKnownGood`.
    pub origin: &'static str,
}

impl From<ReaderApplication> for ReadersRow {
    fn from(application: ReaderApplication) -> Self {
        Self {
            state: match application.state {
                ReaderState::Applied => "applied",
                ReaderState::Pending => "pending",
                ReaderState::Unobserved => "unobserved",
                ReaderState::NoReaders => "noReaders",
            },
            reasons: application.reasons,
            expected: application.expected.map(|stamp| StampRow {
                generation: stamp.generation,
                record_sha256: stamp.sha256.to_string(),
            }),
            observed_at_unix_seconds: application.observed_at_unix_seconds,
            desired_replicas: application.desired_replicas,
            members: application.members.into_iter().map(MemberRow::from).collect(),
        }
    }
}

impl From<MemberFinding> for MemberRow {
    fn from(member: MemberFinding) -> Self {
        let (finding, detail) = match member.finding {
            Finding::Matches => ("matches", None),
            Finding::Behind => ("behind", None),
            Finding::LastKnownGood => ("lastKnownGood", None),
            Finding::Ahead => ("ahead", None),
            Finding::WrongDigest => ("wrongDigest", None),
            Finding::NothingServed => ("nothingServed", None),
            Finding::Failed(failure) => ("failed", Some(failure.to_string())),
            Finding::NotReady => ("notReady", None),
            Finding::Terminating => ("terminating", None),
            Finding::Uncompared => ("uncompared", None),
        };
        Self {
            pod: member.pod,
            replica_set: member.replica_set,
            finding,
            detail,
            served: member.report.map(|report| ServedRow {
                generation: report.generation,
                record_sha256: report.record_sha256.to_string(),
                origin: match report.origin {
                    ServedOrigin::Published => "published",
                    ServedOrigin::LastKnownGood => "lastKnownGood",
                },
            }),
        }
    }
}
