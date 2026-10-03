//! Whether a version of a described component is a release unit: one rule,
//! written once (ADR 0026 section 3).
//!
//! Platform Management's discovery and the catalogue's selection both ask
//! it, and a rule each wrote for itself would be two rules the day one of
//! them changed. Only what each already knows about the component differs,
//! and that is an [`Expectation`], not a second copy.

mod claimed;
mod evaluate;
mod expectation;
mod images;
mod reason;
mod standing;
mod together;

#[cfg(test)]
mod described_tests;
#[cfg(test)]
pub(crate) mod fake_registry_tests;
#[cfg(test)]
mod together_tests;

use std::collections::BTreeMap;

pub use evaluate::evaluate;
pub use reason::{InvalidReason, RevisionOf};
pub(crate) use together::{together, Read};

use crate::ReleaseUnit;

/// What the caller already knows a component must be, which the rule checks
/// the component descriptor against at step 3.
#[derive(Clone, Copy)]
pub enum Expectation<'a> {
    /// Platform Management: the environment pins these roles, each at a
    /// repository, and this one of them as the primary. A component
    /// descriptor naming other roles or repositories is *invalid* for this
    /// environment — however self-consistent — because a write would refuse
    /// it, and saying so at discovery is saying it where it was observed.
    Pinned {
        /// The role whose image carries the component descriptor.
        primary: &'a str,

        /// Registry repositories by role, as pinned.
        repositories: &'a BTreeMap<String, String>,
    },

    /// The catalogue, and the release job: every repository the component
    /// descriptor names must be one this answers `true` for — registered
    /// with a registry, or anything at all for a reader with no registry
    /// list of its own.
    Registered(&'a (dyn Fn(&str) -> bool + Send + Sync)),
}

/// The answer, one of four, and a tag that is not there (ADR 0026 section
/// 3).
///
/// Each of undescribed, incoherent and invalid has its own wording in the
/// console, and none falls through to another: each is a different thing
/// Fabric observed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Evaluation {
    /// The primary repository has no such tag — not one of the four
    /// answers, because there is nothing to describe. A tag that vanished
    /// between a listing and this read is asked about again next pass.
    NotTagged,

    /// A release unit.
    ///
    /// Boxed because it carries the whole component descriptor, and the
    /// other answers carry almost nothing.
    Complete(Box<DescribedRelease>),

    /// The tag exists and no component descriptor is attached to the digest
    /// it resolves to. Fabric cannot tell a publication in progress from one
    /// that will never attach one, and says only what it saw.
    Undescribed,

    /// The images and the component descriptor name different commits, or
    /// another image's version tag is missing or now points at bytes the
    /// component descriptor does not name: one version built twice.
    Incoherent,

    /// A component descriptor is present and cannot be used, for the reason
    /// given.
    Invalid(InvalidReason),
}

/// A complete release unit, and the component descriptor that made it one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DescribedRelease {
    /// The version, the one commit, and every image by role at the digest
    /// Fabric computed.
    pub unit: ReleaseUnit,

    /// The role whose image the component descriptor is attached to.
    pub primary: String,

    /// The component descriptor's own manifest digest — named in a commit
    /// message that writes this release, never in desired state (ADR 0026
    /// section 9).
    pub descriptor_digest: String,

    /// What the component says it is.
    pub descriptor: fabric_component::ComponentDescriptor,
}
