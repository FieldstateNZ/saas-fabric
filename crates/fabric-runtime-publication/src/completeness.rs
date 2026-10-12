//! Whether the target holds one whole recorded generation (G3).
//!
//! An interrupted publication leaves earlier documents written and later
//! ones not. Readers keep their last complete generation, but the target
//! itself is left mixed until a pass completes it. This is what an operator
//! is told about that, read from the target rather than inferred from how
//! the last pass ended: a pass that wrote nothing can leave a complete
//! target, and a restart forgets how the last pass ended.

mod observation;

#[cfg(test)]
mod completeness_tests;

pub use observation::{ObservedDocument, TargetObservation};

use crate::RecordStamp;

/// What one inspection of the target found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PublicationCompleteness {
    /// Every document, and the issuer registry when the record names one,
    /// is held at the revision and digest the held record names.
    Complete {
        /// The record that names them.
        record: RecordStamp,
    },
    /// A held record names a generation the target does not hold whole.
    Incomplete {
        /// The record whose generation is not held whole.
        record: RecordStamp,
        /// Every part that does not match it.
        gaps: Vec<CompletenessGap>,
    },
    /// No publication record is held, so no generation is established.
    Unpublished,
    /// The inspection could not decide.
    Unobserved {
        /// Why.
        reason: UnobservedReason,
    },
}

/// One part of a generation the target does not hold as its record names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompletenessGap {
    /// Which part.
    pub part: RecordedPart,
    /// What is wrong with it.
    pub kind: GapKind,
}

/// A part a publication record names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordedPart {
    /// `tenants.json` and its manifest.
    Tenants,
    /// `data-sources.json` and its manifest.
    DataSources,
    /// `catalog.json` and its manifest.
    Catalog,
    /// `issuer-registry.json`.
    IssuerRegistry,
}

/// How a part differs from what the record names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GapKind {
    /// The payload is absent.
    PayloadMissing,
    /// The payload's SHA-256 is not the one the record names.
    DigestMismatch,
    /// The manifest is absent, even if the record would stand in for it.
    ManifestMissing,
    /// The manifest does not parse, or describes another document.
    ManifestUnreadable,
    /// The manifest states another revision than the record names.
    RevisionMismatch,
}

/// Why an inspection could not decide.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnobservedReason {
    /// Something the inspection needed could not be read.
    Unavailable,
    /// The record changed while the target was read, so what was read may
    /// combine two observations.
    Unstable,
    /// The held record does not parse as a record this publisher reads.
    RecordUnreadable,
}

impl std::fmt::Display for CompletenessGap {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let part = match self.part {
            RecordedPart::Tenants => "tenants",
            RecordedPart::DataSources => "data-sources",
            RecordedPart::Catalog => "catalog",
            RecordedPart::IssuerRegistry => "issuer registry",
        };
        let kind = match self.kind {
            GapKind::PayloadMissing => "payload is missing",
            GapKind::DigestMismatch => "payload does not match the record's digest",
            GapKind::ManifestMissing => "manifest is missing",
            GapKind::ManifestUnreadable => "manifest cannot be read",
            GapKind::RevisionMismatch => "manifest is at another revision than the record names",
        };
        write!(formatter, "{part} {kind}")
    }
}

impl std::fmt::Display for UnobservedReason {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Unavailable => "the publication target could not be read",
            Self::Unstable => "the publication record changed while the target was read",
            Self::RecordUnreadable => "the held publication record cannot be read",
        })
    }
}
