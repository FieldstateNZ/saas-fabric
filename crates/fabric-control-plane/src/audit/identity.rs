//! The audit records for an identity edit and for re-asserting an identity
//! projection.

use fabric_client_model::{ClientId, ClientRevision};
use fabric_core::{event_id, EventType};
use fabric_reconciliation::{ReconciliationOutcome, ReconciliationStatus};

use crate::{AuditRecord, Operator, DOMAIN_ID};

/// A client's identity configuration was changed.
///
/// Info, not debug: this is the record, and a level that a deployment might
/// filter out would make the audit trail a configuration accident.
pub(crate) fn identity_updated(
    operator: &Operator,
    client: &ClientId,
    revision: &ClientRevision,
) -> AuditRecord {
    const EVENT: &str = "control_plane.audit.identity_updated";
    const OPERATION: &str = "update_client_identity";
    tracing::info!(
        event = EVENT,
        event_id = event_id(DOMAIN_ID, EventType::Success, 2),
        operation = OPERATION,
        requested_by = operator.subject(),
        client_id = %client,
        revision = %revision,
        "operator changed a client's identity configuration"
    );
    super::client_revision(EVENT, OPERATION, operator, client, revision)
}

/// An operator re-asserted a client's identity projection, overwriting the
/// unsupported hand edits the provider held (D01-19b).
///
/// Recorded whatever the pass concluded, a failure included: the operator
/// asked for hand edits to be overwritten, and that request is the decision
/// worth recording. A failed re-assertion can still have changed something,
/// so the record says exactly what is known:
///
/// - `overwritten`: hand edits on clients whose update the provider
///   confirmed, including those before a later failure;
/// - `unconfirmed`: hand edits on the client whose update failed, which may
///   or may not have taken effect;
/// - `actions`: how many writes the provider confirmed.
///
/// Each list is `client:field` pairs — application client ids and fixed
/// field names, never a URI or any provider text.
pub(crate) fn identity_projection_reasserted(
    operator: &Operator,
    client: &ClientId,
    revision: &ClientRevision,
    outcome: &ReconciliationOutcome,
    status: ReconciliationStatus,
) -> AuditRecord {
    const EVENT: &str = "control_plane.audit.identity_projection_reasserted";
    const OPERATION: &str = "reassert_identity_projection";
    let joined = |edits: &[fabric_reconciliation::HandEdit]| {
        edits
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(",")
    };
    let overwritten = joined(outcome.overwritten());
    let unconfirmed = joined(outcome.unconfirmed_overwrites());
    tracing::info!(
        event = EVENT,
        event_id = event_id(DOMAIN_ID, EventType::Success, 19),
        operation = OPERATION,
        requested_by = operator.subject(),
        client_id = %client,
        revision = %revision,
        overwritten = %overwritten,
        unconfirmed = %unconfirmed,
        actions = outcome.actions(),
        status = status.as_str(),
        "operator re-asserted a client's identity projection"
    );
    super::client_revision(EVENT, OPERATION, operator, client, revision)
        .with("overwritten", overwritten)
        .with("unconfirmed", unconfirmed)
        .with("actions", outcome.actions())
        .with("status", status.as_str())
}
