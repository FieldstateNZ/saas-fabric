//! Structured log events for the Data API.
//!
//! The field set follows §29. Four rules shape it:
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
//! - **A token-derived value is logged only through
//!   [`fabric_identity::sanitise`].** [`operation_forbidden`]'s
//!   subject is the token's `sub` claim, which nothing in this process
//!   verified: a `sub` carrying a newline turns one audit record into two, and
//!   one carrying a right-to-left override makes a record read as somebody
//!   else. `fabric_identity` is the platform's single enforcement point for
//!   that rule, and this module calls it rather than keeping a second copy.
//!
//! Over the 120-line advisory threshold. The reason is that this is one set of
//! typed emitters for one domain's events, each a few lines of `tracing` call
//! behind a name and the argument for what it is safe to put on the line. The
//! four rules above are stated once, here, and apply to every one of them;
//! splitting the emitters across files would leave a reader deciding which
//! half the rules were about. The one pair that is genuinely its own concept —
//! the two halves of a connector failure — is already in `request_failure`.

use fabric_connector::ExecutionTarget;
use fabric_core::{event_id, EventType, LogicalDataSourceName, LogicalResourceName, TenantId};
use fabric_identity::sanitise;

use crate::DOMAIN_ID;

mod request_failure;

pub(crate) use request_failure::{connector_refused, request_failed};

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
///
/// `subject` is the token's `sub` claim, so it is sanitised and bounded before
/// it reaches the line — see this module's fourth rule. `subject_truncated`
/// and `subject_filtered` ride with it, because a subject cut at the bound and
/// a subject that is genuinely that long are otherwise the same record, and so
/// are a subject the filter emptied and a token that carried no `sub` at all.
pub(crate) fn operation_forbidden(resource: &str, operation: &str, subject: &str) {
    let subject = sanitise(subject);

    tracing::warn!(
        event = "data_api.operation_forbidden",
        event_id = event_id(DOMAIN_ID, EventType::Warning, 1),
        logical_resource = resource,
        operation,
        subject = %subject,
        subject_truncated = subject.truncated,
        subject_filtered = subject.filtered,
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
