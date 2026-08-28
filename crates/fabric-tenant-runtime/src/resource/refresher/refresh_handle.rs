//! Controlling a running refresher.

use std::sync::Arc;

use tokio::sync::Notify;
use tokio::task::JoinHandle;

/// Controls a background refresher.
///
/// The composition root holds one per registry until shutdown. Dropping it
/// orphans the task — the loop keeps polling with no way to stop it.
pub struct RefreshHandle {
    trigger: Arc<Notify>,
    shutdown: Arc<Notify>,
    task: JoinHandle<()>,
}

impl RefreshHandle {
    /// Wraps a spawned refresh task.
    pub(crate) const fn new(trigger: Arc<Notify>, shutdown: Arc<Notify>, task: JoinHandle<()>) -> Self {
        Self {
            trigger,
            shutdown,
            task,
        }
    }

    /// Asks the refresher to reload now rather than waiting for the interval.
    ///
    /// Returns immediately; the reload happens on the background task. Several
    /// triggers arriving together coalesce into one reload, which is what you
    /// want when a reconciler updates twenty resources in a burst.
    pub fn refresh_now(&self) {
        self.trigger.notify_one();
    }

    /// Stops the refresher and waits for it to finish.
    ///
    /// # Errors
    ///
    /// Returns the join error if the background task panicked.
    pub async fn shutdown(self) -> Result<(), tokio::task::JoinError> {
        self.shutdown.notify_one();
        self.task.await
    }
}
