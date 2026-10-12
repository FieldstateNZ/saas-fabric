//! A client's lifecycle moved: tombstoned, or its deletion confirmed.

use fabric_client_model::{ClientId, ClientRevision};
use fabric_core::{event_id, EventType};

use crate::{AuditRecord, Operator, DOMAIN_ID};

/// A client was tombstoned. Its state is retained; nothing was removed.
pub(crate) fn client_tombstoned(
    operator: &Operator,
    client: &ClientId,
    revision: &ClientRevision,
) -> AuditRecord {
    const EVENT: &str = "control_plane.audit.client_tombstoned";
    const OPERATION: &str = "tombstone_client";
    tracing::info!(
        event = EVENT,
        event_id = event_id(DOMAIN_ID, EventType::Success, 16),
        operation = OPERATION,
        requested_by = operator.subject(),
        client_id = %client,
        revision = %revision,
        "operator tombstoned a client"
    );
    super::client_revision(EVENT, OPERATION, operator, client, revision)
}

/// Deletion of a tombstoned client was confirmed. This records intent only:
/// nothing was deleted.
pub(crate) fn client_deletion_confirmed(
    operator: &Operator,
    client: &ClientId,
    revision: &ClientRevision,
) -> AuditRecord {
    const EVENT: &str = "control_plane.audit.client_deletion_confirmed";
    const OPERATION: &str = "confirm_client_deletion";
    tracing::info!(
        event = EVENT,
        event_id = event_id(DOMAIN_ID, EventType::Success, 17),
        operation = OPERATION,
        requested_by = operator.subject(),
        client_id = %client,
        revision = %revision,
        "operator confirmed deletion of a tombstoned client"
    );
    super::client_revision(EVENT, OPERATION, operator, client, revision)
}
