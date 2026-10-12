//! What the log pipeline records about the lifecycle continuity store.

use fabric_client_model::ClientId;
use fabric_core::{event_id, EventType};

use crate::{ContinuityHold, ContinuityRefusal, DOMAIN_ID};

/// The continuity store refused or failed a read or a reservation. The
/// detail stays here; the operator sees a fixed sentence.
pub(crate) fn continuity_refused(client: &ClientId, refusal: &ContinuityRefusal) {
    tracing::warn!(
        event = "control_plane.continuity.refused",
        event_id = event_id(DOMAIN_ID, EventType::Warning, 10),
        client_id = %client,
        error = %refusal,
        "the continuity store refused a lifecycle transition; nothing was written"
    );
}

/// A pass left a client's identity untouched because the continuity store
/// does not confirm its lifecycle.
pub(crate) fn identity_held(client: &ClientId, hold: ContinuityHold) {
    tracing::warn!(
        event = "control_plane.continuity.identity_held",
        event_id = event_id(DOMAIN_ID, EventType::Warning, 11),
        client_id = %client,
        reason = %hold,
        "a client's identity was not converged: its lifecycle is not confirmed"
    );
}

/// The document write failed in a way that does not prove nothing was
/// written, so the reservation is left pending rather than released.
pub(crate) fn continuity_write_unconfirmed(client: &ClientId, reservation: &str) {
    tracing::error!(
        event = "control_plane.continuity.write_unconfirmed",
        event_id = event_id(DOMAIN_ID, EventType::Error, 8),
        client_id = %client,
        reservation,
        "a lifecycle write failed and may have landed; its reservation is still pending"
    );
}

/// The document recorded a transition and the store could not record that
/// it did. The reservation stays pending, and the client held, until the
/// same request is sent again.
pub(crate) fn continuity_completion_failed(
    client: &ClientId,
    reservation: &str,
    refusal: &ContinuityRefusal,
) {
    tracing::error!(
        event = "control_plane.continuity.completion_failed",
        event_id = event_id(DOMAIN_ID, EventType::Error, 7),
        client_id = %client,
        reservation,
        error = %refusal,
        "a lifecycle transition was written and its reservation is still pending"
    );
}
