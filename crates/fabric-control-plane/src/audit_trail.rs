//! The durable audit trail: the authoritative record of what operators did
//! (ADR 0028).
//!
//! # Why this exists beside the activity feed
//!
//! The activity feed lives inside desired state, is editable by anyone with
//! write access to that repository, and is bounded by the document size
//! limit. It is a convenience view. This module is the record an
//! investigation reads: append-only, hash-chained per stream so a changed,
//! dropped or reordered entry is detectable, and bounded per entry so it can
//! never be grown by one oversized record.
//!
//! # The shape
//!
//! ```text
//! audit::<event>()  ──►  AuditRecord  ──►  AuditRecorder::record
//!                                              │ (disabled: log pipeline only)
//!                                              ▼
//!                                      dyn AuditTrail::append
//!                                              │ seals into the stream's chain
//!                                              ▼
//!                                         AuditEntry
//! ```
//!
//! Nothing here prunes, trims or expires anything. Retention and where the
//! trail is stored in a live environment are deferred decisions (ADR 0028,
//! "Deferred"); until they are made, a deployment either leaves the trail
//! disabled or points it at a local development directory.

mod chain_break;
mod chain_hash;
mod entry;
#[cfg(test)]
mod entry_tests;
mod errors;
mod head;
mod in_memory;
mod port;
mod record;
mod recorder;
mod stream;
mod verify;
#[cfg(test)]
mod verify_tests;

pub use chain_break::ChainBreak;
pub use chain_hash::ChainHash;
pub use entry::AuditEntry;
pub use errors::AuditError;
pub use head::ChainHead;
pub use in_memory::InMemoryAuditTrail;
pub use port::{AuditTrail, StoredStream};
pub use record::AuditRecord;
pub use recorder::AuditRecorder;
pub use stream::AuditStream;
pub use verify::verify;

/// The largest encoded entry, in bytes, that any trail accepts.
///
/// Every record carries validated names, revisions, digests and stable
/// codes, which encode to a few hundred bytes. 4 KiB is ample headroom for
/// that and small enough that free text slipping into a record is refused
/// rather than stored. A record over it is refused with
/// [`AuditError::EntryTooLarge`]; it is never shortened to fit, because a
/// shortened record would be a different record carrying a valid hash.
pub const MAX_AUDIT_ENTRY_BYTES: usize = 4 * 1024;
