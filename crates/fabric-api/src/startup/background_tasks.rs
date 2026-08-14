//! The background tasks the application owns, and stopping them.
//!
//! # Why these are grouped
//!
//! Three tasks are spawned before [`build`](super::build) can know whether the
//! rest of the graph will assemble: two registry refreshers and the connector
//! retry loop. Every handle's own docs say the same thing — dropping it
//! *orphans* its task rather than stopping it — and `build` used to do exactly
//! that on any later failure, returning `Err` from the catalogue load, the
//! connector negotiation, or the Data API build and letting the handles fall
//! out of scope.
//!
//! In the binary that is short-lived: the process exits moments later. It is
//! not short-lived anywhere else — a test, or any host embedding `build` and
//! carrying on — and "the process is about to die anyway" is not a property
//! the composition root should be relying on.
//!
//! Grouping them here means the stop path is written once and used by both the
//! failure path and the ordinary shutdown in `main`, rather than twice with a
//! chance to diverge.

use fabric_tenant_runtime::RuntimeHandles;

use crate::startup::ConnectorRetryHandle;

/// Everything still running behind the served router.
pub struct BackgroundTasks {
    refresh: RuntimeHandles,
    connector_retry: ConnectorRetryHandle,
}

impl BackgroundTasks {
    /// Takes ownership of the handles. Called only by
    /// [`build`](super::build).
    pub(super) const fn new(refresh: RuntimeHandles, connector_retry: ConnectorRetryHandle) -> Self {
        Self {
            refresh,
            connector_retry,
        }
    }

    /// Stops every task and waits for it.
    ///
    /// Both failures are logged rather than returned. The caller is shutting
    /// down and has nothing left to do differently, and stopping the second
    /// task matters more than reporting that the first one panicked.
    pub async fn shutdown(self) {
        stop_refreshers(self.refresh).await;
        stop_connector_retry(self.connector_retry).await;
    }
}

/// Stops both registry refreshers, logging a task that panicked.
pub(super) async fn stop_refreshers(refresh: RuntimeHandles) {
    if let Err(error) = refresh.shutdown().await {
        tracing::warn!(event = "fabric.refresher_shutdown_failed", reason = %error);
    }
}

/// Stops the connector retry loop, logging a task that panicked.
pub(super) async fn stop_connector_retry(retry: ConnectorRetryHandle) {
    if let Err(error) = retry.shutdown().await {
        tracing::warn!(event = "fabric.connector_retry_shutdown_failed", reason = %error);
    }
}
