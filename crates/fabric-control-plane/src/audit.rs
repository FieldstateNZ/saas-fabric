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
//! revision that resulted. The timestamp comes from the log pipeline rather
//! than a field here, because a record that carried its own idea of the time
//! could disagree with the line above it.
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

use fabric_client_model::{ClientId, ClientRevision};
use fabric_core::{event_id, EventType};

use crate::{Operator, DOMAIN_ID};

/// A client's identity configuration was changed.
///
/// Info, not debug: this is the record, and a level that a deployment might
/// filter out would make the audit trail a configuration accident.
pub(crate) fn identity_updated(operator: &Operator, client: &ClientId, revision: &ClientRevision) {
    tracing::info!(
        event = "control_plane.audit.identity_updated",
        event_id = event_id(DOMAIN_ID, EventType::Success, 2),
        operation = "update_client_identity",
        requested_by = operator.subject(),
        client_id = %client,
        revision = %revision,
        "operator changed a client's identity configuration"
    );
}
