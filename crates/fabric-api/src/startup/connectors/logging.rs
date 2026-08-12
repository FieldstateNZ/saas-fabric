//! Structured log events for connector startup and retry.

/// A connector could not be negotiated at startup.
///
/// Error, not warning: until the background retry succeeds, every tenant
/// bound to this connector is unservable. Naming the connector and the
/// reason here is what lets an operator triage without waiting for the first
/// affected request to fail (§35).
pub(super) fn negotiation_failed(connector: &str, reason: &str) {
    tracing::error!(
        event = "fabric.connector_negotiation_failed",
        connector,
        reason,
        "connector could not be negotiated at startup; it will be retried in the background"
    );
}

/// A background retry attempt failed again.
///
/// Warn, not error: this is the retry loop working as designed, and the
/// original failure was already logged at `error` once. Repeating that level
/// on every interval would bury the one log line that actually needs
/// attention under a stream of expected retries.
pub(super) fn retry_failed(connector: &str, reason: &str) {
    tracing::warn!(
        event = "fabric.connector_retry_failed",
        connector,
        reason,
        "connector retry did not succeed; will try again next interval"
    );
}

/// A previously-unavailable connector was negotiated successfully.
pub(super) fn connector_recovered(connector: &str) {
    tracing::info!(
        event = "fabric.connector_recovered",
        connector,
        "connector negotiated after previously failing at startup"
    );
}
