//! The audit record for an operator-triggered publication pass.

use fabric_core::{event_id, EventType};

use crate::{AuditRecord, AuditStream, Operator, DOMAIN_ID};

/// An operator triggered a runtime-publication pass (ADR 0023 part 4), and
/// what it did.
///
/// # Why this names `resource = "runtime-publication"` rather than a client
///
/// Publication is environment-wide, not any one client's document -- the
/// same reason [`catalogue_changed`](super::catalogue_changed) names
/// `resource = "catalogue"` instead of a `client_id`.
///
/// # Why no revision travels with the record
///
/// A pass may advance any of three documents by a different amount, or
/// none at all, and `outcome` already says which case this was. `GET
/// /api/platform` carries the actual revisions for whoever wants them; this
/// record is the trail that an operator asked for a pass and what it found,
/// not a second copy of the numbers.
///
/// Only the operator trigger calls this. The scheduled pass carries no
/// operator to attribute, and an unattended pass is not the human decision
/// this trail exists to record (see `crate::audit`'s own rustdoc).
pub(crate) fn publication_triggered(operator: &Operator, outcome: &str) -> AuditRecord {
    const EVENT: &str = "control_plane.audit.publication_triggered";
    const OPERATION: &str = "publish_runtime_state";
    tracing::info!(
        event = EVENT,
        event_id = event_id(DOMAIN_ID, EventType::Success, 13),
        operation = OPERATION,
        requested_by = operator.subject(),
        resource = "runtime-publication",
        outcome,
        "operator triggered a runtime-publication pass"
    );
    AuditRecord::new(AuditStream::Publication, EVENT, OPERATION, operator.subject()).with("outcome", outcome)
}
