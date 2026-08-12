//! Structured log events for the tenant runtime.

use fabric_core::{event_id, BindingRevision, EventType, TenantId};

use crate::{ApplyReport, DOMAIN_ID};

/// A request arrived before the registry had loaded anything.
///
/// Warning rather than error: it is expected briefly during a cold start. A
/// sustained stream of these means the binding source is not reachable and the
/// process should not be in the load balancer.
pub(crate) fn resolve_before_prime(tenant: &TenantId) {
    tracing::warn!(
        event = "tenant_runtime.resolve_before_prime",
        event_id = event_id(DOMAIN_ID, EventType::Warning, 1),
        tenant_id = %tenant,
        "tenant resolution attempted before the registry was primed; returning service unavailable"
    );
}

/// A request named a tenant the registry does not hold.
pub(crate) fn unknown_tenant(tenant: &TenantId) {
    tracing::warn!(
        event = "tenant_runtime.unknown_tenant",
        event_id = event_id(DOMAIN_ID, EventType::Warning, 2),
        tenant_id = %tenant,
        "no runtime binding for tenant; rejecting"
    );
}

/// An incoming binding was older than the one already held.
///
/// Worth noticing: it usually means two sources are publishing, or one is
/// reading a replica that has fallen behind.
pub(crate) fn stale_binding_ignored(tenant: &TenantId, incoming: BindingRevision, held: BindingRevision) {
    tracing::warn!(
        event = "tenant_runtime.stale_binding_ignored",
        event_id = event_id(DOMAIN_ID, EventType::Warning, 3),
        tenant_id = %tenant,
        incoming_revision = incoming.get(),
        held_revision = held.get(),
        "ignoring a binding older than the one held"
    );
}

/// A new snapshot was installed.
pub(crate) fn snapshot_applied(tenant_count: usize, report: &ApplyReport) {
    if report.is_noop() {
        tracing::debug!(
            event = "tenant_runtime.snapshot_unchanged",
            event_id = event_id(DOMAIN_ID, EventType::Debug, 1),
            tenant_count,
            "refresh produced no binding changes"
        );
        return;
    }

    tracing::info!(
        event = "tenant_runtime.snapshot_applied",
        event_id = event_id(DOMAIN_ID, EventType::Success, 1),
        tenant_count,
        added = report.added,
        updated = report.updated,
        removed = report.removed,
        stale_ignored = report.stale_ignored,
        "installed a new tenant binding snapshot"
    );
}

/// The registry loaded bindings for the first time.
pub(crate) fn primed(source: &str, tenant_count: usize) {
    tracing::info!(
        event = "tenant_runtime.primed",
        event_id = event_id(DOMAIN_ID, EventType::Success, 2),
        source,
        tenant_count,
        "tenant runtime primed and ready to serve"
    );
}

/// A refresh failed.
///
/// Error rather than warning, and deliberately explicit that the previous
/// snapshot is still serving — the single most useful thing for whoever reads
/// this at three in the morning.
pub(crate) fn refresh_failed(source: &str, error: &dyn std::error::Error) {
    tracing::error!(
        event = "tenant_runtime.refresh_failed",
        event_id = event_id(DOMAIN_ID, EventType::Error, 1),
        source,
        reason = %error,
        "failed to refresh tenant bindings; continuing to serve the last good snapshot"
    );
}

/// The background refresher started.
pub(crate) fn refresher_started(source: &str, interval_seconds: u64) {
    tracing::info!(
        event = "tenant_runtime.refresher_started",
        event_id = event_id(DOMAIN_ID, EventType::Success, 3),
        source,
        interval_seconds,
        "tenant binding refresher started"
    );
}

/// The background refresher stopped.
pub(crate) fn refresher_stopped(source: &str) {
    tracing::info!(
        event = "tenant_runtime.refresher_stopped",
        event_id = event_id(DOMAIN_ID, EventType::Success, 4),
        source,
        "tenant binding refresher stopped"
    );
}
