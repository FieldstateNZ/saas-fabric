//! What the recovery command reports about each held part. Digests,
//! revisions and reasons only; never document content.

use crate::{DocumentRevision, RecordedDocument, RecordedRegistry};

/// The held publication record.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum RecordState {
    /// None is held.
    Absent,
    /// It parses, and this publisher may write over it.
    Held {
        /// Its generation.
        generation: u64,
        /// The SHA-256 of its bytes.
        sha256: String,
    },
    /// It does not parse, or its rollback floor is above this publisher.
    Unusable {
        /// Why, as a publication refusal says it.
        detail: String,
        /// The SHA-256 of its bytes.
        sha256: String,
    },
}

/// A held manifest.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum ManifestState {
    /// None is held.
    Absent,
    /// This document's manifest, at a supported contract.
    Held {
        /// The revision it states.
        revision: DocumentRevision,
    },
    /// This document's manifest, from a newer publisher contract.
    Newer {
        /// The revision it states.
        revision: DocumentRevision,
        /// The contract that stamped it.
        contract_version: u32,
    },
    /// Not a JSON object.
    Malformed {
        /// Why.
        detail: String,
    },
    /// Readable, but another document's or of an unknown shape.
    Conflicting {
        /// Why.
        detail: String,
    },
}

/// One held document.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct DocumentDiagnosis {
    /// Its manifest.
    pub manifest: ManifestState,
    /// The SHA-256 of its payload, if one is held.
    pub payload_sha256: Option<String>,
    /// What the held record names for it, if a usable record is held.
    pub recorded: Option<RecordedDocument>,
    /// Whether the held payload is the bytes the record names.
    pub matches_record: Option<bool>,
}

/// The held issuer registry.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct RegistryDiagnosis {
    /// What the held record names, if a usable record names one.
    pub recorded: Option<RecordedRegistry>,
    /// The SHA-256 of the held registry, if one is held.
    pub held_sha256: Option<String>,
    /// The generation the held registry states, if it parses.
    pub held_generation: Option<u64>,
}
