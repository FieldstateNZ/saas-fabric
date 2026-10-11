//! A client's lifecycle moved: tombstoned, or its deletion confirmed.

use fabric_client_model::{ClientId, ClientRevision};
use fabric_core::{event_id, EventType};

use crate::{Operator, DOMAIN_ID};

/// A client was tombstoned. Its state is retained; nothing was removed.
pub(crate) fn client_tombstoned(operator: &Operator, client: &ClientId, revision: &ClientRevision) {
    tracing::info!(
        event = "control_plane.audit.client_tombstoned",
        event_id = event_id(DOMAIN_ID, EventType::Success, 16),
        operation = "tombstone_client",
        requested_by = operator.subject(),
        client_id = %client,
        revision = %revision,
        "operator tombstoned a client"
    );
}

/// Deletion of a tombstoned client was confirmed. This records intent only:
/// nothing was deleted.
pub(crate) fn client_deletion_confirmed(operator: &Operator, client: &ClientId, revision: &ClientRevision) {
    tracing::info!(
        event = "control_plane.audit.client_deletion_confirmed",
        event_id = event_id(DOMAIN_ID, EventType::Success, 17),
        operation = "confirm_client_deletion",
        requested_by = operator.subject(),
        client_id = %client,
        revision = %revision,
        "operator confirmed deletion of a tombstoned client"
    );
}
