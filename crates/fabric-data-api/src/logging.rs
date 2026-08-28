//! Structured log events for the Data API.
//!
//! The field set follows §29. Three rules shape it:
//!
//! - **Application-facing concepts stay logical.** `tenant_id`,
//!   `logical_resource`, `logical_data_source`, and `operation` are the
//!   vocabulary an application itself could recognise — the same names it
//!   uses when it makes the request.
//! - **Physical resource information stays in platform telemetry.**
//!   `data_source_id`, `data_source_revision`, `tenant_binding_revision`, and
//!   `physical_resource_identifier` are emitted here because this *is*
//!   platform telemetry; none of the four ever appears in a response.
//! - **Secrets and connection strings never appear.** Not in a field, not in a
//!   message, not inside an error being formatted. `ResolvedSecret` cannot print
//!   itself, which makes that structurally hard to get wrong.

use fabric_connector::ExecutionTarget;
use fabric_core::{event_id, EventType, LogicalDataSourceName, LogicalResourceName, TenantId};

use crate::DOMAIN_ID;

/// An operation was dispatched to a connector.
///
/// Debug level: one line per data operation at info would drown a busy
/// deployment's logs. The trace context carries the same information for
/// requests that are actually sampled.
pub(crate) fn operation_dispatched(
    resource: &LogicalResourceName,
    logical_data_source: &LogicalDataSourceName,
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
        logical_data_source = %logical_data_source,
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
/// Its counterpart is [`request_failed`], which covers the 5xx half. Between
/// them every connector failure is recorded exactly once, which matters
/// because the caller receives a replaced message in both cases: `detail` is
/// the only surviving copy of what the connector actually said.
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

/// A request named a tenant this runtime does not know.
///
/// Externally indistinguishable from a disabled or never-provisioned tenant —
/// all three answer the same 403 with the same message, which is the
/// anti-enumeration measure. Internally they are not the same event: this one
/// exists so an operator can see a stream of these and recognise probing,
/// which a caller watching only status codes cannot learn is happening.
pub(crate) fn unknown_tenant_probed(tenant: &TenantId, request_id: &str) {
    tracing::warn!(
        event = "data_api.unknown_tenant_probed",
        event_id = event_id(DOMAIN_ID, EventType::Warning, 3),
        tenant_id = %tenant,
        request_id,
        "a request named a tenant unknown to the runtime"
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
