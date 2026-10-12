//! What the recovery command reports about each held part.

use crate::{DocumentRevision, RecordedDocument};

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
    /// Why it refuses every publication, if it does.
    pub blocking: Option<String>,
}

/// A held manifest.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum ManifestState {
    /// None is held.
    Absent,
    /// It parses and names this document.
    Held {
        /// The revision it states.
        revision: DocumentRevision,
    },
    /// It does not parse, or names another document.
    Unreadable {
        /// Why.
        detail: String,
    },
}
