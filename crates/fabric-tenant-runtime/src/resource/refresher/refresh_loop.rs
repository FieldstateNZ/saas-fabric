//! The timer, trigger and shutdown wiring every refresher shares.

use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::Notify;

use super::RefreshHandle;
use crate::RuntimeConfig;

/// Runs `pass` every interval, or sooner when triggered, until shut down,
/// then runs `stopped` once. `pass` is handed the interval as the longest
/// any one reload in it may take.
///
/// What a pass reloads is the caller's business: one registry, or several in
/// a fixed order. Keeping the loop itself in one place means the poll, the
/// trigger and the shutdown behave identically however many registries a
/// pass covers.
pub(super) fn spawn<P, F>(
    config: &RuntimeConfig,
    pass: P,
    stopped: impl FnOnce() + Send + 'static,
) -> RefreshHandle
where
    P: Fn(Duration) -> F + Send + 'static,
    F: Future<Output = ()> + Send,
{
    let interval = Duration::from_secs(config.refresh_interval_seconds);
    let trigger = Arc::new(Notify::new());
    let shutdown = Arc::new(Notify::new());

    let task_trigger = Arc::clone(&trigger);
    let task_shutdown = Arc::clone(&shutdown);

    let task = tokio::spawn(async move {
        loop {
            tokio::select! {
                () = tokio::time::sleep(interval) => {}
                () = task_trigger.notified() => {}
                () = task_shutdown.notified() => break,
            }

            pass(interval).await;
        }

        stopped();
    });

    RefreshHandle::new(trigger, shutdown, task)
}
