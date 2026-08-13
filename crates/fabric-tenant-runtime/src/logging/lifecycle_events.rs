//! Log events about a registry or refresher itself.

use fabric_core::{event_id, EventType};

use crate::resource::{ApplyReport, RegistryResource};
use crate::DOMAIN_ID;

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
        invalid_rejected = report.invalid_rejected,
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
