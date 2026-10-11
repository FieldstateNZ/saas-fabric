//! What a publication record says about each document of its generation.

use crate::digest::sha256_hex;
use crate::DocumentRevision;

/// The three documents of one generation.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecordedDocuments {
    /// `tenants.json`.
    pub tenants: RecordedDocument,
    /// `data-sources.json`.
    #[serde(rename = "data-sources")]
    pub data_sources: RecordedDocument,
    /// `catalog.json`.
    pub catalog: RecordedDocument,
}

/// One document as a generation names it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecordedDocument {
    /// The document's revision, as its manifest states it.
    pub revision: DocumentRevision,
    /// The lowercase hex SHA-256 of the payload bytes exactly as published.
    pub sha256: String,
}

impl RecordedDocument {
    /// Names `bytes` at `revision`.
    #[must_use]
    pub fn of(revision: DocumentRevision, bytes: &[u8]) -> Self {
        Self {
            revision,
            sha256: sha256_hex(bytes),
        }
    }
}
