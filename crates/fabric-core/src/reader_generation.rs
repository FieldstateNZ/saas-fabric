//! What a runtime replica says it serves, in the shape the control plane
//! reads it (G7).
//!
//! The two planes must not depend on each other and must still agree on
//! this shape, so it lives in the crate both may share, as
//! [`OperationKind`](crate::OperationKind) does. Each side brings its own
//! JSON codec; this crate holds the types and the rules a decoded report
//! must meet.
//!
//! The report is deliberately small: the generation number, the SHA-256 of
//! the publication record that names it, and whether the replica read it
//! from the published files or from its last-known-good copy. It carries no
//! tenant, issuer, data-source, path or token, because the listener that
//! serves it is unauthenticated.

use serde::{Deserialize, Serialize};

use crate::RecordDigest;

/// The version of [`ReaderGenerationReport`]'s shape. A reader of the
/// report refuses any other.
pub const READER_GENERATION_CONTRACT: u32 = 1;

/// Where the report is served, on the replica's separate metadata listener.
pub const READER_GENERATION_PATH: &str = "/reader-generation";

/// The most bytes a report may take. A well-formed one is under 200; a
/// caller stops reading at this bound and treats the reply as malformed.
pub const READER_GENERATION_MAX_BYTES: usize = 1024;

/// The generation one replica's resolution reads now, taken from the one
/// snapshot that resolution reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReaderGenerationReport {
    /// [`READER_GENERATION_CONTRACT`] when written.
    pub contract: u32,
    /// The publication record's generation number.
    pub generation: u64,
    /// The SHA-256 of the record's exact bytes. Two records at one
    /// generation number that differ are two generations.
    pub record_sha256: RecordDigest,
    /// Where the replica read it from.
    pub origin: ServedOrigin,
}

/// Where a served generation was read from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServedOrigin {
    /// The published files, matching their record.
    Published,
    /// The replica's persisted last-known-good copy, held because no
    /// published generation at or above it could be applied.
    LastKnownGood,
}

/// Why a decoded report cannot be used.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ReportRefusal {
    /// The report is written at a contract this side does not read.
    #[error(
        "the report is at contract {0}; this reader understands only contract {READER_GENERATION_CONTRACT}"
    )]
    UnsupportedContract(u32),
    /// Generation 0 is never published: records start at 1.
    #[error("the report names generation 0, which no publication record carries")]
    NoGeneration,
}

impl ReaderGenerationReport {
    /// A report at this contract.
    #[must_use]
    pub const fn new(generation: u64, record_sha256: RecordDigest, origin: ServedOrigin) -> Self {
        Self {
            contract: READER_GENERATION_CONTRACT,
            generation,
            record_sha256,
            origin,
        }
    }

    /// This report, if it is one a reader may compare.
    ///
    /// # Errors
    ///
    /// [`ReportRefusal`] for another contract or generation 0.
    pub const fn checked(self) -> Result<Self, ReportRefusal> {
        if self.contract != READER_GENERATION_CONTRACT {
            return Err(ReportRefusal::UnsupportedContract(self.contract));
        }
        if self.generation == 0 {
            return Err(ReportRefusal::NoGeneration);
        }
        Ok(self)
    }
}

#[cfg(test)]
#[path = "reader_generation_tests.rs"]
mod tests;
