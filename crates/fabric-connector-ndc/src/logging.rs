//! Structured log events for the NDC connector.

use fabric_core::{event_id, EventType};

use crate::DOMAIN_ID;

/// A connector was negotiated successfully at startup.
pub(crate) fn connector_ready(connector: &str, version: &str, collections: usize, writable: bool) {
    tracing::info!(
        event = "ndc.connector_ready",
        event_id = event_id(DOMAIN_ID, EventType::Success, 1),
        connector,
        ndc_version = version,
        collections,
        writable,
        "NDC connector negotiated and ready"
    );
}

/// The connector implements a later patch version than this client requires.
///
/// Warning rather than error, because a connector *ahead* of our floor is
/// compatible: additions at patch level are gated behind capabilities this
/// client does not claim. The drift is still worth an operator's attention,
/// which is what this line is for.
///
/// Note the asymmetry, which is the whole point of the floor: a connector
/// *behind* the floor never reaches this line. It is rejected at startup,
/// because the wire format is not stable within a minor version and 0.2.4
/// added the request-level arguments every tenant's routing rides on — see
/// `registration::version`.
pub(crate) fn version_ahead_of_floor(connector: &str, connector_version: &str, minimum_version: &str) {
    tracing::warn!(
        event = "ndc.version_ahead_of_floor",
        event_id = event_id(DOMAIN_ID, EventType::Warning, 1),
        connector,
        connector_version,
        minimum_version,
        "connector implements a later NDC patch version than this client requires; continuing"
    );
}

/// An operation was refused before it reached the connector.
///
/// `reason` must come from
/// [`ConnectorError::operator_message`](fabric_connector::ConnectorError::operator_message),
/// not from `Display`. A refusal's physical specifics — the collection, field,
/// or procedure it was raised over — are deliberately kept out of the error's
/// `Display` so nothing can forward them to an application, which means this
/// log line is where they surface or nowhere.
pub(crate) fn operation_refused(connector: &str, operation: &str, reason: &str) {
    tracing::warn!(
        event = "ndc.operation_refused",
        event_id = event_id(DOMAIN_ID, EventType::Warning, 2),
        connector,
        operation,
        reason,
        "refusing an operation this connector cannot express faithfully"
    );
}

/// A connector rejected an operation.
///
/// The connector's own message is logged here and **only** here — it can name
/// physical tables and servers, so it never travels back to an application
/// (§29).
pub(crate) fn connector_rejected(connector: &str, operation: &str, message: &str) {
    tracing::error!(
        event = "ndc.connector_rejected",
        event_id = event_id(DOMAIN_ID, EventType::Error, 1),
        connector,
        operation,
        message,
        "connector rejected an operation"
    );
}
