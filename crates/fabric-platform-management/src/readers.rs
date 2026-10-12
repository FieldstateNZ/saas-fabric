//! Whether the runtime's replicas serve the generation the publisher
//! published (G7).
//!
//! The publisher's evidence ends at the publication target: the readiness
//! gates say what was published, never what a replica resolves against.
//! This module compares the two. A [`ReaderObserver`] samples every replica
//! that could be serving, each one's report taken from the view its
//! resolution reads, and [`assess`] holds that sample to the record the last
//! accepted pass stamped ([`fabric_runtime_publication::RecordStamp`]).
//!
//! The answer is one of four, and only one of them is a positive claim:
//!
//! - **applied**: in this sample, every replica that could serve reports
//!   exactly the stamped generation and record digest, read from the
//!   published files. Evidence about that sample, not about a later or an
//!   in-flight request, and not about a token or a session.
//! - **pending**: a replica is behind, on its last-known-good copy, not yet
//!   ready, or terminating. Expected to move on its own.
//! - **unobserved**: nothing was sampled, the sample was not trustworthy,
//!   or a replica answered something that cannot be compared: ahead of the
//!   record, another record at the same generation, nothing, or garbage.
//! - **no readers**: replicas are deliberately zero. Not applied, and not
//!   evidence that anything was withdrawn.
//!
//! No transport here: the Kubernetes adapter implements the port.

mod assess;
mod finding;
mod reasons;
mod sample;

#[cfg(test)]
#[path = "readers/assess_tests.rs"]
mod assess_tests;

pub use assess::{assess, update_complete, ReaderApplication, ReaderState};
pub use finding::{Finding, MemberFinding};
pub use sample::{
    MemberState, ProbeFailure, ProbeOutcome, ReaderMember, ReaderObserver, ReaderSample, SampleOutcome,
};
