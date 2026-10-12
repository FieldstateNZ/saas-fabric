//! The audit record for an identity edit.

use fabric_client_model::{ClientId, ClientRevision};
use fabric_core::{event_id, EventType};

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
