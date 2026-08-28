//! Controlling a running reconciliation loop.

use std::sync::Arc;

use tokio::sync::Notify;
use tokio::task::JoinHandle;

/// Controls the background reconciliation loop.
///
/// The composition root holds this until shutdown. Dropping it orphans the
/// task — the loop keeps sweeping with no way to stop it, which in a test
/// means a provider being called after the test believed it had finished.
pub struct ReconciliationLoopHandle {
    /// Asks the loop to stop.
    shutdown: Arc<Notify>,

    /// The running task.
    task: JoinHandle<()>,
}

impl ReconciliationLoopHandle {
    /// Wraps a spawned loop.
    pub(crate) const fn new(shutdown: Arc<Notify>, task: JoinHandle<()>) -> Self {
        Self { shutdown, task }
    }

    /// Stops the loop and waits for the current sweep to finish.
    ///
    /// # Errors
    ///
    /// Returns the join error if the background task panicked.
    pub async fn shutdown(self) -> Result<(), tokio::task::JoinError> {
        self.shutdown.notify_one();
        self.task.await
    }
}
