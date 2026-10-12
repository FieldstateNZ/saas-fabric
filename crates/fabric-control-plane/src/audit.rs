//! The record of what an operator changed.
//!
//! # Why this is not just another log module
//!
//! `logging` records what the process did. This records **what a human
//! decided**, and the two have different audiences and different retention
//! expectations. Keeping them apart means an audit query is a filter on one
//! event name rather than an archaeology exercise across every line the
//! service emitted (§24).
//!
//! # What every record carries
//!
//! Who requested it, which client, which domain operation, when, and the
//! revision that resulted. A log line's timestamp comes from the log pipeline
//! rather than a field here, because a line that carried its own idea of the
//! time could disagree with the pipeline's; a durable entry is stamped once,
//! by the recorder, when it is sealed into its chain.
//!
//! # What no record may ever carry
//!
//! A secret, a token, an administrative credential, or the contents of one.
//! Nothing in this module is handed a value that could contain one — the
//! parameters are an operator subject, a client id, and a revision, all of
//! which are validated types.
//!
//! # Git is part of the trail, not the whole of it
//!
//! A Git-backed repository also records the change as a commit, and that is a
//! genuinely useful second copy. It is not sufficient on its own: commits are
//! authored by the platform's machine identity, a future repository may not be
//! Git at all, and a refused write leaves no commit but is still worth
//! knowing about.
//!
//! # Every record has two copies, and neither is the activity feed
//!
//! Each function here writes its log line and returns the same values as an
//! [`AuditRecord`](crate::AuditRecord), which the caller hands to the
//! [`AuditRecorder`](crate::AuditRecorder) — the durable, hash-chained trail
//! when a deployment has one (ADR 0028). The activity feed an operator sees
//! in the console is a bounded view kept in desired state; it is not either
//! copy, and nothing here reads from it except the catalogue's own entry name.

mod identity;
mod lifecycle;
mod product;
mod publication;
mod registry;
mod secret;
mod selection;

pub(crate) use identity::{identity_projection_reasserted, identity_updated};
pub(crate) use lifecycle::{client_deletion_confirmed, client_tombstoned};
pub(crate) use product::{catalogue_changed, client_created, product_updated};
pub(crate) use publication::publication_triggered;
pub(crate) use registry::{registry_changed, RegistryOperation};
pub(crate) use secret::client_secret;
pub(crate) use selection::{component_selected, Selecting, SelectionOutcome};

use fabric_client_model::{ClientId, ClientRevision};

use crate::{AuditRecord, AuditStream, Operator};

/// The record shared by every event that names an operator, a client and the
/// revision its write produced.
fn client_revision(
    event: &str,
    operation: &str,
    operator: &Operator,
    client: &ClientId,
    revision: &ClientRevision,
) -> AuditRecord {
    AuditRecord::new(
        AuditStream::Client(client.clone()),
        event,
        operation,
        operator.subject(),
    )
    .with("revision", revision)
}
