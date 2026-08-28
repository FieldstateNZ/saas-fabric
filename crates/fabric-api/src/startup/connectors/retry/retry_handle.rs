//! Controlling the running connector retry task.

use std::sync::Arc;

use tokio::sync::Notify;
use tokio::task::JoinHandle;

/// Controls the background connector-retry loop.
///
/// The composition root holds this until shutdown, mirroring
/// [`fabric_tenant_runtime::RefreshHandle`]: dropping it would orphan the
/// task rather than stop it, leaving it polling a process that is otherwise
/// shutting down.
pub struct ConnectorRetryHandle {
    shutdown: Arc<Notify>,
    task: JoinHandle<()>,
}

impl ConnectorRetryHandle {
    /// Wraps a spawned retry task.
    pub(super) const fn new(shutdown: Arc<Notify>, task: JoinHandle<()>) -> Self {
        Self { shutdown, task }
    }

    /// Stops the retry loop and waits for it to finish.
    ///
    /// # Errors
    ///
    /// Returns the join error if the background task panicked.
    pub async fn shutdown(self) -> Result<(), tokio::task::JoinError> {
        self.shutdown.notify_one();
        self.task.await
    }
}
