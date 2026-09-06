//! The two events that record why a request failed, split by status class.
//!
//! Their own file because they are one concept read twice. Between them every
//! connector failure is recorded exactly once, and the reason that matters is
//! the same for both: the caller receives a replaced message, so `detail` here
//! is the only surviving copy of what the connector actually said. Written
//! apart from the emitters beside them, the pairing is easy to break — a third
//! failure path added to one and not the other, and a class of failure stops
//! being recorded anywhere.

use fabric_core::{event_id, EventType};

use crate::DOMAIN_ID;

/// A request failed with a server error.
///
/// The single place every 5xx is recorded with its internal detail — the
/// caller only receives a generic message, so if it is not logged here it is
/// lost. `request_id` is the same id the caller was given in the response
/// (§29, item 57), so a report that quotes it is one grep away from this
/// line.
pub(crate) fn request_failed(code: &str, detail: &str, request_id: &str) {
    tracing::error!(
        event = "data_api.request_failed",
        event_id = event_id(DOMAIN_ID, EventType::Error, 1),
        code,
        detail,
        request_id,
        "data API request failed"
    );
}

/// A connector refused an operation and the caller was told a 4xx.
///
/// Warn rather than error: the request is refused cleanly and nothing is
/// broken in this process. Warn rather than debug because none of the reasons
/// are the caller's fault — an unmapped collection, an operation the catalogue
/// describes but the backend cannot express — so each is an operator's signal
/// that a catalogue entry and a backend have drifted apart.
///
/// Its counterpart is [`request_failed`], which covers the 5xx half.
pub(crate) fn connector_refused(code: &str, detail: &str, request_id: &str) {
    tracing::warn!(
        event = "data_api.connector_refused",
        event_id = event_id(DOMAIN_ID, EventType::Warning, 4),
        code,
        detail,
        request_id,
        "a connector refused the operation"
    );
}
