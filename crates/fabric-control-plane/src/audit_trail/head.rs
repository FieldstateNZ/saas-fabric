//! Where a chain ends.

use crate::audit_trail::ChainHash;

/// The last entry of a stream: its sequence and its hash.
///
/// Kept by an adapter beside the stream as an anchor. The chain alone proves
/// each entry follows the one before it, but cannot say an entry is missing
/// from the end; comparing against a head recorded at append time can. Each
/// append also writes the head to the log pipeline, which is a second anchor
/// outside the store (ADR 0028, "What the chain does and does not prove").
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChainHead {
    /// The last entry's sequence, counting from one.
    pub sequence: u64,
    /// The last entry's hash.
    pub hash: ChainHash,
}
