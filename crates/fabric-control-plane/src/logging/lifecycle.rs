//! What the log pipeline records about a restore's continuity store.

use fabric_client_model::ClientId;
use fabric_core::{event_id, EventType};

use crate::{ContinuityRefusal, DOMAIN_ID};

/// The continuity store refused or failed a reservation, so nothing was
/// written. The detail stays here; the operator sees a fixed sentence.
pub(crate) fn restore_reservation_refused(client: &ClientId, refusal: &ContinuityRefusal) {
    tracing::warn!(
        event = "control_plane.restore.reservation_refused",
        event_id = event_id(DOMAIN_ID, EventType::Warning, 10),
        client_id = %client,
        error = %refusal,
        "the continuity store refused a restoration; nothing was written"
    );
}

/// The document write failed in a way that does not prove nothing was
/// written, so the reservation is left pending rather than released.
pub(crate) fn restore_write_unconfirmed(client: &ClientId, reservation: &str) {
    tracing::error!(
        event = "control_plane.restore.write_unconfirmed",
        event_id = event_id(DOMAIN_ID, EventType::Error, 8),
        client_id = %client,
        reservation,
        "a restoration's document write failed and may have landed; its reservation is still pending"
    );
}

/// The document recorded a restoration and the store could not record that
/// it did. The reservation stays pending for the store to resolve.
pub(crate) fn restore_completion_failed(client: &ClientId, reservation: &str, refusal: &ContinuityRefusal) {
    tracing::error!(
        event = "control_plane.restore.completion_failed",
        event_id = event_id(DOMAIN_ID, EventType::Error, 7),
        client_id = %client,
        reservation,
        error = %refusal,
        "a restoration was written and its continuity reservation is still pending"
    );
}
