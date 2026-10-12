//! A client's lifecycle moved: tombstoned, its deletion confirmed, or a
//! tombstone reversed.

use fabric_client_model::lifecycle::Tombstone;
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

/// A tombstone was reversed. The client is saved intent again, with the
/// tombstone kept in its history; nothing was re-created or regenerated.
///
/// Names the reversed tombstone by when it was written, never by its reason:
/// a reason is the operator's free text, and the trail refuses free text.
pub(crate) fn client_restored(
    operator: &Operator,
    client: &ClientId,
    revision: &ClientRevision,
    reversed: &Tombstone,
) -> AuditRecord {
    const EVENT: &str = "control_plane.audit.client_restored";
    const OPERATION: &str = "restore_client";
    tracing::info!(
        event = EVENT,
        event_id = event_id(DOMAIN_ID, EventType::Success, 20),
        operation = OPERATION,
        requested_by = operator.subject(),
        client_id = %client,
        revision = %revision,
        tombstoned_at = reversed.at,
        "operator restored a tombstoned client"
    );
    super::client_revision(EVENT, OPERATION, operator, client, revision).with("tombstonedAt", reversed.at)
}
