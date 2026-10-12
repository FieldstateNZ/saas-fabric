//! How a stored chain failed verification.

use crate::audit_trail::ChainHead;

/// Where a stream stopped being the chain that was sealed.
///
/// Each variant is a distinct thing to investigate, so they are kept apart
/// rather than reported as "invalid".
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ChainBreak {
    /// An entry's content no longer matches its own hash: it was edited.
    #[error("entry {sequence} does not match its hash")]
    Tampered {
        /// The edited entry.
        sequence: u64,
    },

    /// An entry names a predecessor other than the entry before it.
    #[error("entry {sequence} does not follow the entry before it")]
    BrokenLink {
        /// The entry whose link is wrong.
        sequence: u64,
    },

    /// A sequence number is absent and never appears later: an entry was
    /// dropped.
    #[error("entry {expected} is missing (found {found} in its place)")]
    Missing {
        /// The sequence that should have come next.
        expected: u64,
        /// What came instead.
        found: u64,
    },

    /// The expected sequence appears, but later: entries were reordered.
    #[error("entry {expected} is out of order (found {found} in its place)")]
    OutOfOrder {
        /// The sequence that should have come next.
        expected: u64,
        /// What came instead.
        found: u64,
    },

    /// An entry belongs to a different stream.
    #[error("entry {sequence} belongs to another stream")]
    ForeignEntry {
        /// The misplaced entry.
        sequence: u64,
    },

    /// The stream ends before the head recorded for it: its tail was cut off.
    #[error("the stream ends at {found} but its recorded head is {}", anchor.sequence)]
    Truncated {
        /// The recorded head.
        anchor: ChainHead,
        /// The last sequence present, zero for an empty stream.
        found: u64,
    },

    /// The entry at the recorded head's sequence has a different hash: the
    /// chain was rewritten from some point and re-hashed.
    #[error("entry {} does not match the recorded head", anchor.sequence)]
    AnchorMismatch {
        /// The recorded head.
        anchor: ChainHead,
    },
}
