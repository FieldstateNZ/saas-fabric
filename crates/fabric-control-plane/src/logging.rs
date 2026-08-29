//! Structured log events for the control plane.
//!
//! Three rules, all of them about what must not appear:
//!
//! - **No credentials.** Not the repository's token, not the identity
//!   provider's, not an operator's. Nothing in this module is handed one.
//! - **No upstream response bodies.** A repository or provider failure has
//!   already been reduced to a sanitised sentence by the adapter that raised
//!   it; that sentence is what is logged.
//! - **No repository internals in anything an operator also sees.** A path or
//!   a branch may appear in the adapter's own `describe`, which is operator
//!   telemetry; it never reaches a response body (§8).
//!
//! Audit records — what a human changed — are deliberately not here. See
//! [`audit`](crate::audit).

mod integration;

pub(crate) use integration::*;

use fabric_client_model::ClientId;
use fabric_core::{event_id, EventType};
use fabric_reconciliation::ReconciliationStatus;

use crate::repository::RepositoryError;
use crate::DOMAIN_ID;

/// The control plane finished starting.
pub(crate) fn control_plane_ready(repository: &str, operators: &str) {
    tracing::info!(
        event = "control_plane.ready",
        event_id = event_id(DOMAIN_ID, EventType::Success, 1),
        repository,
        operators,
        "control plane ready"
    );
}

/// The reconciliation loop started.
pub(crate) fn reconciliation_started(repository: &str, interval_seconds: u64) {
    tracing::info!(
        event = "control_plane.reconciliation_started",
        event_id = event_id(DOMAIN_ID, EventType::Success, 3),
        repository,
        interval_seconds,
        "reconciliation loop started"
    );
}

/// The reconciliation loop stopped.
pub(crate) fn reconciliation_stopped() {
    tracing::info!(
        event = "control_plane.reconciliation_stopped",
        event_id = event_id(DOMAIN_ID, EventType::Success, 4),
        "reconciliation loop stopped"
    );
}

/// One client was reconciled.
///
/// Debug for the ordinary case, but drift and failure are worth seeing, so the
/// level is chosen from the status rather than fixed. A `drifted` line is the
/// only evidence anywhere that something outside SaaS Fabric is editing a
/// realm the platform owns.
pub(crate) fn client_reconciled(client: &ClientId, status: ReconciliationStatus, actions: usize) {
    match status {
        ReconciliationStatus::Applied | ReconciliationStatus::Pending => tracing::debug!(
            event = "control_plane.client_reconciled",
            event_id = event_id(DOMAIN_ID, EventType::Debug, 1),
            client_id = %client,
            status = status.as_str(),
            actions,
            "client reconciled"
        ),
        ReconciliationStatus::Drifted => tracing::warn!(
            event = "control_plane.client_drifted",
            event_id = event_id(DOMAIN_ID, EventType::Warning, 1),
            client_id = %client,
            actions,
            "a converged client had diverged and was corrected"
        ),
        ReconciliationStatus::Failed => tracing::error!(
            event = "control_plane.client_reconciliation_failed",
            event_id = event_id(DOMAIN_ID, EventType::Error, 2),
            client_id = %client,
            "a client could not be reconciled"
        ),
    }
}

/// A sweep could not read desired state.
pub(crate) fn sweep_failed(repository: &str, error: &RepositoryError) {
    // Nothing configured is a state, not a fault. Logging it at `error` every
    // interval would fill the log of a perfectly healthy platform that is
    // simply waiting for an operator, and would train whoever reads it to
    // ignore the level that matters.
    if matches!(error, RepositoryError::NotConfigured) {
        tracing::debug!(
            event = "control_plane.sweep_skipped",
            "no desired-state repository is configured; nothing to reconcile"
        );
        return;
    }

    tracing::error!(
        event = "control_plane.sweep_failed",
        event_id = event_id(DOMAIN_ID, EventType::Error, 3),
        repository,
        detail = %error,
        "could not read desired state; recorded reconciliation status is unchanged"
    );
}

/// An identity was presented that is not a platform operator.
///
/// The subject is deliberately absent. It is attacker-controlled in the case
/// that matters, and a log that echoed it would let anyone write arbitrary
/// text into the platform's audit stream. The header name is enough to tell a
/// misconfigured proxy from a rejected human.
pub(crate) fn operator_refused(header: &str) {
    tracing::warn!(
        event = "control_plane.operator_refused",
        event_id = event_id(DOMAIN_ID, EventType::Warning, 2),
        header,
        "an identity was presented that is not a platform operator"
    );
}

/// A request failed with a server error.
///
/// The single place every 5xx is recorded with its internal detail — the
/// operator receives a shorter message, so if it is not written here it is
/// lost.
pub(crate) fn request_failed(code: &str, detail: &str) {
    tracing::error!(
        event = "control_plane.request_failed",
        event_id = event_id(DOMAIN_ID, EventType::Error, 1),
        code,
        detail,
        "control plane request failed"
    );
}
