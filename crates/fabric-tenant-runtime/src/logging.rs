//! Structured log events for the runtime plane.
//!
//! The registry lifecycle is generic over the resource type, so these helpers
//! are too: `T::KIND` supplies the `resource_kind` field, which is what lets a
//! single set of events cover both tenant bindings and data sources without
//! either losing its identity in the logs.

use fabric_core::{event_id, BindingRevision, EventType};

use crate::resource::{ApplyReport, RegistryResource};
use crate::DOMAIN_ID;

/// An incoming resource was older than the one already held.
///
/// Worth noticing: it usually means two sources are publishing, or one is
/// reading a replica that has fallen behind.
pub(crate) fn stale_resource_ignored<T: RegistryResource>(
    key: &T::Key,
    incoming: BindingRevision,
    held: BindingRevision,
) {
    tracing::warn!(
        event = "runtime.stale_resource_ignored",
        event_id = event_id(DOMAIN_ID, EventType::Warning, 3),
        resource_kind = T::KIND,
        resource_key = %key,
        incoming_revision = incoming.get(),
        held_revision = held.get(),
        "ignoring a resource older than the one held"
    );
}

/// A new snapshot was installed.
pub(crate) fn snapshot_applied<T: RegistryResource>(count: usize, report: &ApplyReport) {
    if report.is_noop() {
        tracing::debug!(
            event = "runtime.snapshot_unchanged",
            event_id = event_id(DOMAIN_ID, EventType::Debug, 1),
            resource_kind = T::KIND,
            count,
            "refresh produced no changes"
        );
        return;
    }

    tracing::info!(
        event = "runtime.snapshot_applied",
        event_id = event_id(DOMAIN_ID, EventType::Success, 1),
        resource_kind = T::KIND,
        count,
        added = report.added,
        updated = report.updated,
        removed = report.removed,
        stale_ignored = report.stale_ignored,
        "installed a new snapshot"
    );
}

/// A registry loaded for the first time.
pub(crate) fn primed<T: RegistryResource>(source: &str, count: usize) {
    tracing::info!(
        event = "runtime.primed",
        event_id = event_id(DOMAIN_ID, EventType::Success, 2),
        resource_kind = T::KIND,
        source,
        count,
        "registry primed"
    );
}

/// A refresh failed.
///
/// Error rather than warning, and deliberately explicit that the previous
/// snapshot is still serving — the most useful thing for whoever reads this at
/// three in the morning.
pub(crate) fn refresh_failed<T: RegistryResource>(source: &str, error: &dyn std::error::Error) {
    tracing::error!(
        event = "runtime.refresh_failed",
        event_id = event_id(DOMAIN_ID, EventType::Error, 1),
        resource_kind = T::KIND,
        source,
        reason = %error,
        "refresh failed; continuing to serve the last good snapshot"
    );
}

/// A background refresher started.
pub(crate) fn refresher_started<T: RegistryResource>(source: &str, interval_seconds: u64) {
    tracing::info!(
        event = "runtime.refresher_started",
        event_id = event_id(DOMAIN_ID, EventType::Success, 3),
        resource_kind = T::KIND,
        source,
        interval_seconds,
        "refresher started"
    );
}

/// A background refresher stopped.
pub(crate) fn refresher_stopped<T: RegistryResource>(source: &str) {
    tracing::info!(
        event = "runtime.refresher_stopped",
        event_id = event_id(DOMAIN_ID, EventType::Success, 4),
        resource_kind = T::KIND,
        source,
        "refresher stopped"
    );
}
