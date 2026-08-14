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

/// The connector implements a different specification version than this client.
///
/// Warning rather than error for a patch difference: the wire format is stable
/// within a minor version, and refusing to start over a patch mismatch would be
/// needlessly brittle. A minor or major difference is rejected at startup
/// instead — see `registration`.
pub(crate) fn version_patch_mismatch(connector: &str, connector_version: &str, client_version: &str) {
    tracing::warn!(
        event = "ndc.version_patch_mismatch",
        event_id = event_id(DOMAIN_ID, EventType::Warning, 1),
        connector,
        connector_version,
        client_version,
        "connector implements a different NDC patch version; continuing"
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
