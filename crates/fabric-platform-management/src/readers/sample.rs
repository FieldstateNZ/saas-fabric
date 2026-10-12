//! What one bounded sample of the runtime's replicas found.

use async_trait::async_trait;
use fabric_core::ReaderGenerationReport;

/// Samples every replica that could be serving, read-only.
#[async_trait]
pub trait ReaderObserver: Send + Sync {
    /// One sample. Never an error: a sample that cannot be trusted is
    /// [`SampleOutcome::Unavailable`], which assesses as unobserved.
    async fn sample(&self) -> ReaderSample;
}

/// One sample, and when it finished.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReaderSample {
    /// When the sample finished, as seconds since the Unix epoch.
    pub observed_at_unix_seconds: u64,
    /// What it found.
    pub outcome: SampleOutcome,
}

/// What a sample found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SampleOutcome {
    /// The workload could not be read, or changed while it was read: its
    /// membership, ownership or identity moved between the first read and
    /// the last.
    Unavailable {
        /// A controlled explanation, never a provider response body.
        detail: String,
    },
    /// The workload and every pod it owns, through every one of its
    /// replica sets, unchanged from the first read to the last.
    Observed {
        /// The replicas the workload asks for.
        desired_replicas: u32,
        /// Whether the workload's controller has observed its current spec.
        rollout_observed: bool,
        /// Every owned pod, from current and older replica sets alike.
        members: Vec<ReaderMember>,
    },
}

/// One pod the workload owns.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReaderMember {
    /// The pod's name.
    pub pod: String,
    /// The name of the replica set that owns it.
    pub replica_set: String,
    /// What it was found doing.
    pub state: MemberState,
}

/// Whether a pod was asked, and what it said.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MemberState {
    /// Not ready: not asked, and not serving until it is.
    NotReady,
    /// Being deleted: not asked, and possibly still finishing requests.
    Terminating,
    /// Ready, and asked.
    Probed(ProbeOutcome),
}

/// What a ready pod answered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProbeOutcome {
    /// A report it serves.
    Reported(ReaderGenerationReport),
    /// It holds no whole recorded generation (`404`).
    NothingServed,
    /// No usable answer.
    Failed(ProbeFailure),
}

/// Why a ready pod gave no usable answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProbeFailure {
    /// The pod has no address to ask.
    NoAddress,
    /// The connection failed.
    Unreachable,
    /// It did not answer in time.
    TimedOut,
    /// It answered with a redirect, which is never followed.
    Redirected,
    /// It answered with another status.
    Status(u16),
    /// Its answer exceeded the size bound.
    TooLarge,
    /// Its answer was not a report this side reads.
    Malformed,
}
