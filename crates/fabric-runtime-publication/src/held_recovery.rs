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
//! document of its generation. [`plan_restore`] puts bytes with that digest
//! back at that revision, and nothing else:
//!
//! - no revision changes, so nothing rolls back (D01-1, roll forward only);
//! - the record, its generation, floors, history and registry are untouched;
//! - only a missing or malformed manifest is rebuilt; readable metadata of
//!   another kind, revision or contract is refused;
//! - the offered document must keep every resource rule, alone and against
//!   each usable peer, and every unclear peer is reported, since one repair
//!   never establishes the generation as a whole.
//!
//! A digest match is agreement with the record held now. It is not signed
//! history, and it does not stop a writer able to replace record and payload
//! together.
//!
//! No discard is offered (#108): the next publication would plan against an
//! empty document, and its tenant-removal, emptying and retirement guards
//! could not see what was discarded.

mod diagnosis;
#[cfg(test)]
mod fixtures;
mod held_parse;
mod held_parts;
mod held_state;
mod port;
mod problem;
mod refusal;
mod restore_checks;
mod restore_plan;
mod semantics;

pub use diagnosis::{diagnose, HeldDiagnosis};
pub use held_state::{DocumentDiagnosis, ManifestState, RecordState, RegistryDiagnosis};
pub use port::{HeldObservation, HeldRecovery, RecoveryAudit, RecoveryError};
pub use problem::{Part, Problem, ProblemKind};
pub use refusal::RestoreRefusal;
pub use restore_plan::{plan_restore, RestorePlan};
