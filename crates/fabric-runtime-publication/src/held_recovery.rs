//! Recovering a held tenants or data-sources document the publisher cannot
//! parse (gap G5, publisher side).
//!
//! [`crate::plan_publication`] parses the held tenants and data-sources
//! documents before anything else, and refuses every publication while
//! either does not parse ([`crate::PublicationError::Unreadable`]) or a
//! manifest is held with no payload
//! ([`crate::PublicationError::HeldPayloadLost`]), at any revision. This is
//! the supported way out.
//!
//! # Restore, and only restore
//!
//! The held publication record names the exact SHA-256 and revision of each
//! document of the last complete generation. Bytes with that digest are the
//! bytes that were published, wherever they were kept (a reader's
//! last-known-good copy holds them). [`plan_restore`] puts those bytes back
//! at the revision the record names and nothing else:
//!
//! - no revision changes, so nothing rolls back (D01-1, roll forward only);
//! - every guard the next publication runs reads exactly what the record
//!   attests, so none is weakened;
//! - only a document that blocks publication is touched, and only when the
//!   record vouches for the bytes.
//!
//! Discarding a held document, which no record can vouch for, is not
//! offered: the next publication would plan against an empty document and
//! its tenant-removal, emptying and retirement guards could not see what was
//! discarded.

mod diagnosis;
mod held_parse;
mod held_state;
mod port;
mod refusal;
mod restore_plan;

pub use diagnosis::{diagnose, HeldDiagnosis};
pub use held_state::{DocumentDiagnosis, ManifestState, RecordState};
pub use port::{HeldRecovery, RecoveryAudit, RecoveryError};
pub use refusal::RestoreRefusal;
pub use restore_plan::{plan_restore, RestorePlan};
