//! Why an audit append or read failed.

use crate::audit_trail::{AuditStream, ChainBreak};

/// Everything that can go wrong appending to or reading an audit trail.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AuditError {
    /// The encoded entry is larger than the trail accepts. Refused whole: an
    /// audit record is never shortened to fit.
    #[error("audit entry of {size} bytes exceeds the {limit}-byte limit")]
    EntryTooLarge {
        /// The encoded size that was refused.
        size: usize,
        /// The limit it exceeded.
        limit: usize,
    },

    /// A stored stream does not verify. Appends to it are refused, because a
    /// new entry chained onto a broken stream would lend it a valid head.
    #[error("audit stream {stream} does not verify: {broken}")]
    Broken {
        /// The stream that failed.
        stream: AuditStream,
        /// Where and how.
        broken: ChainBreak,
    },

    /// A stored line is not an entry at all.
    #[error("an audit entry could not be read: {detail}")]
    Malformed {
        /// What the decoder reported.
        detail: String,
    },

    /// The store could not be reached or written. A retry may succeed.
    #[error("the audit trail is unavailable: {detail}")]
    Unavailable {
        /// What failed, without any record content.
        detail: String,
    },
}
