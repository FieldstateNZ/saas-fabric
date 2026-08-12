//! Keeping a registry current, in the background.

mod refresh_handle;
#[cfg(test)]
mod refresher_tests;

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::Notify;

use crate::resource::{RegistryResource, ResourceRegistry, ResourceSource};
use crate::{logging, RuntimeConfig, SourceError};

pub use refresh_handle::RefreshHandle;

/// Loads resources into a registry, once at startup and then periodically.
///
/// # Why polling *and* a trigger
///
/// The trigger ([`RefreshHandle::refresh_now`]) is the fast path: a reconciler
/// that has just changed something tells the runtime, and the change propagates
/// in milliseconds. That is what makes a migration cut-over (§19) feel instant.
///
/// The poll is the safety net. Notifications get lost — a pod restarts
/// mid-flight, a webhook 500s, a partition eats it. Without a poll, one lost
/// notification strands a resource on stale state indefinitely and nothing ever
/// notices. With one, staleness is bounded by the interval regardless.
///
/// Neither is Git in the request path (§6): both write into the registry ahead
/// of the requests that read it.
pub struct ResourceRefresher;

impl ResourceRefresher {
    /// Loads once, so the registry can serve.
    ///
    /// # Errors
    ///
    /// [`SourceError`] if the source could not be read. The caller decides
    /// whether that is fatal — see [`RuntimeConfig::fail_fast_on_prime`].
    pub async fn prime<T: RegistryResource>(
        registry: &ResourceRegistry<T>,
        source: &dyn ResourceSource<T>,
    ) -> Result<usize, SourceError> {
        let resources = source.load().await?;
        let count = resources.len();

        registry.apply_all(resources);
        logging::primed::<T>(&source.describe(), count);

        Ok(count)
    }

    /// Starts the background refresh loop.
    #[must_use]
    pub fn spawn<T: RegistryResource>(
        registry: Arc<ResourceRegistry<T>>,
        source: Arc<dyn ResourceSource<T>>,
        config: &RuntimeConfig,
    ) -> RefreshHandle {
        let interval = Duration::from_secs(config.refresh_interval_seconds);
        let trigger = Arc::new(Notify::new());
        let shutdown = Arc::new(Notify::new());

        let task_trigger = Arc::clone(&trigger);
        let task_shutdown = Arc::clone(&shutdown);
        let description = source.describe();

        logging::refresher_started::<T>(&description, config.refresh_interval_seconds);

        let task = tokio::spawn(async move {
            loop {
                tokio::select! {
                    () = tokio::time::sleep(interval) => {}
                    () = task_trigger.notified() => {}
                    () = task_shutdown.notified() => break,
                }

                match source.load().await {
                    Ok(resources) => {
                        registry.apply_all(resources);
                    }
                    Err(error) => {
                        // Deliberately does not touch the registry. The last
                        // good snapshot keeps serving; a momentarily unreadable
                        // source must not deprovision everything.
                        logging::refresh_failed::<T>(&source.describe(), &error);
                    }
                }
            }

            logging::refresher_stopped::<T>(&description);
        });

        RefreshHandle::new(trigger, shutdown, task)
    }
}
