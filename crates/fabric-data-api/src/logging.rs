//! Structured log events for the Data API.
//!
//! The field set follows §29. Two rules shape it:
//!
//! - **Physical resource information stays in platform telemetry.** The
//!   `physical_resource_identifier` field is emitted here because this *is*
//!   platform telemetry; it never appears in a response.
//! - **Secrets and connection strings never appear.** Not in a field, not in a
//!   message, not inside an error being formatted. `ResolvedSecret` cannot print
//!   itself, which makes that structurally hard to get wrong.

use fabric_connector::ExecutionTarget;
use fabric_core::{event_id, EventType, LogicalResourceName};

use crate::DOMAIN_ID;

/// An operation was dispatched to a connector.
///
/// Debug level: one line per data operation at info would drown a busy
/// deployment's logs. The trace context carries the same information for
/// requests that are actually sampled.
pub(crate) fn operation_dispatched(
    resource: &LogicalResourceName,
    operation: &str,
    target: &ExecutionTarget,
) {
    tracing::debug!(
        event = "data_api.operation_dispatched",
        event_id = event_id(DOMAIN_ID, EventType::Debug, 1),
        tenant_id = %target.tenant(),
        tenant_binding_revision = target.tenant_revision().get(),
        data_source_id = %target.data_source(),
        data_source_revision = target.data_source_revision().get(),
        logical_resource = %resource,
        operation,
        physical_resource_identifier = target.physical_resource_identifier(),
        "dispatching a data operation"
    );
}

/// An operation was refused by authorization.
///
/// Warning, not debug: a stream of these is either a misconfigured client or
/// someone probing, and both are worth seeing.
pub(crate) fn operation_forbidden(resource: &str, operation: &str, subject: &str) {
    tracing::warn!(
        event = "data_api.operation_forbidden",
        event_id = event_id(DOMAIN_ID, EventType::Warning, 1),
        logical_resource = resource,
        operation,
        subject,
        "identity is not permitted to perform this operation"
    );
}

/// A write was refused because the DataSource does not accept writes.
///
/// The DataSource label is logged — it is the thing an operator needs — and
/// never returned to the caller, who is told only that the resource is
/// read-only (§2, §29).
pub(crate) fn write_refused_by_data_source(resource: &LogicalResourceName, data_source: &str) {
    tracing::warn!(
        event = "data_api.write_refused_by_data_source",
        event_id = event_id(DOMAIN_ID, EventType::Warning, 2),
        logical_resource = %resource,
        data_source,
        "refusing a write: this data source is not writable"
    );
}

/// A request failed with a server error.
///
/// The single place every 5xx is recorded with its internal detail — the
/// caller only receives a generic message, so if it is not logged here it is
/// lost.
pub(crate) fn request_failed(code: &str, detail: &str) {
    tracing::error!(
        event = "data_api.request_failed",
        event_id = event_id(DOMAIN_ID, EventType::Error, 1),
        code,
        detail,
        "data API request failed"
    );
}

/// The Data API was configured at startup.
pub(crate) fn data_api_ready(resources: usize, connectors: usize) {
    tracing::info!(
        event = "data_api.ready",
        event_id = event_id(DOMAIN_ID, EventType::Success, 1),
        resources,
        connectors,
        "Data API ready"
    );
}
