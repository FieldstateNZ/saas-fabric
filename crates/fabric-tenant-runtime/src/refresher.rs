//! Keeping the registry current, in the background.

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::Notify;
use tokio::task::JoinHandle;

use crate::{logging, BindingSource, BindingSourceError, TenantRuntimeConfig, TenantRuntimeRegistry};

/// Loads bindings into the registry, once at startup and then periodically.
///
/// # Why polling *and* a trigger
///
/// The trigger ([`RefreshHandle::refresh_now`]) is the fast path: a reconciler
/// that has just changed a tenant tells the runtime immediately, and the change
/// propagates in milliseconds. That is what makes a migration cut-over (§19)
/// feel instant.
///
/// The poll is the safety net. Notifications get lost — a pod restarts mid-flight,
/// a webhook 500s, a network partition eats it. Without a poll, a lost
/// notification means a tenant stays on a stale binding indefinitely, and
/// nothing ever notices. With one, staleness is bounded by the interval no
/// matter what.
///
/// Neither is Git in the request path (§6): both write into the registry ahead
/// of the requests that read it.
pub struct BindingRefresher;

impl BindingRefresher {
    /// Loads bindings once, so the registry can serve.
    ///
    /// # Errors
    ///
    /// [`BindingSourceError`] if the source could not be read. The caller
    /// decides whether that is fatal — see
    /// [`TenantRuntimeConfig::fail_fast_on_prime`].
    pub async fn prime(
        registry: &TenantRuntimeRegistry,
        source: &dyn BindingSource,
    ) -> Result<usize, BindingSourceError> {
        let bindings = source.load().await?;
        let count = bindings.len();

        registry.apply_all(bindings);
        logging::primed(&source.describe(), count);

        Ok(count)
    }

    /// Starts the background refresh loop.
    ///
    /// The returned [`RefreshHandle`] both triggers immediate refreshes and
    /// stops the loop when dropped-in-shutdown is not enough — see
    /// [`RefreshHandle::shutdown`].
    #[must_use]
    pub fn spawn(
        registry: Arc<TenantRuntimeRegistry>,
        source: Arc<dyn BindingSource>,
        config: &TenantRuntimeConfig,
    ) -> RefreshHandle {
        let interval = Duration::from_secs(config.refresh_interval_seconds);
        let trigger = Arc::new(Notify::new());
        let shutdown = Arc::new(Notify::new());

        let task_trigger = Arc::clone(&trigger);
        let task_shutdown = Arc::clone(&shutdown);
        let description = source.describe();

        logging::refresher_started(&description, config.refresh_interval_seconds);

        let task = tokio::spawn(async move {
            loop {
                tokio::select! {
                    () = tokio::time::sleep(interval) => {}
                    () = task_trigger.notified() => {}
                    () = task_shutdown.notified() => break,
                }

                match source.load().await {
                    Ok(bindings) => {
                        registry.apply_all(bindings);
                    }
                    Err(error) => {
                        // Deliberately does not touch the registry. The last
                        // good snapshot keeps serving; a momentarily
                        // unreadable source must not deprovision every tenant.
                        logging::refresh_failed(&source.describe(), &error);
                    }
                }
            }

            logging::refresher_stopped(&description);
        });

        RefreshHandle {
            trigger,
            shutdown,
            task,
        }
    }
}

/// Controls a running refresher.
pub struct RefreshHandle {
    trigger: Arc<Notify>,
    shutdown: Arc<Notify>,
    task: JoinHandle<()>,
}

impl RefreshHandle {
    /// Asks the refresher to reload now rather than waiting for the interval.
    ///
    /// Returns immediately; the reload happens on the background task. Several
    /// triggers arriving together coalesce into one reload, which is what you
    /// want when a reconciler updates twenty tenants in a burst.
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

#[cfg(test)]
mod tests {
    use fabric_core::{BindingRevision, TenantId};

    use super::*;
    use crate::{InMemoryBindingSource, TenantRuntimeBinding};

    fn tenant(name: &str) -> TenantId {
        TenantId::try_new(name).unwrap()
    }

    fn binding_at(name: &str, revision: u64) -> TenantRuntimeBinding {
        TenantRuntimeBinding::new(tenant(name), BindingRevision::new(revision))
    }

    fn config() -> TenantRuntimeConfig {
        TenantRuntimeConfig {
            // Long enough that these tests exercise the trigger, not the poll.
            refresh_interval_seconds: 3600,
            fail_fast_on_prime: true,
        }
    }

    #[tokio::test]
    async fn priming_makes_the_registry_servable() {
        let registry = TenantRuntimeRegistry::new();
        let source = InMemoryBindingSource::new(vec![binding_at("acme", 1)]);

        let count = BindingRefresher::prime(&registry, &source).await.unwrap();

        assert_eq!(count, 1);
        assert!(registry.is_primed());
        assert!(registry.resolve(&tenant("acme")).is_ok());
    }

    #[tokio::test]
    async fn priming_from_a_failing_source_leaves_the_registry_unprimed() {
        let registry = TenantRuntimeRegistry::new();
        let source = InMemoryBindingSource::empty();
        source.fail_next();

        assert!(BindingRefresher::prime(&registry, &source).await.is_err());
        assert!(!registry.is_primed());
    }

    #[tokio::test]
    async fn a_triggered_refresh_picks_up_a_new_revision() {
        let registry = Arc::new(TenantRuntimeRegistry::new());
        let source = Arc::new(InMemoryBindingSource::new(vec![binding_at("acme", 1)]));

        BindingRefresher::prime(&registry, source.as_ref()).await.unwrap();

        let mut changes = registry.subscribe();
        let handle = BindingRefresher::spawn(
            Arc::clone(&registry),
            Arc::clone(&source) as Arc<dyn BindingSource>,
            &config(),
        );

        source.set(vec![binding_at("acme", 2)]);
        handle.refresh_now();

        let change = changes.recv().await.unwrap();
        assert_eq!(change.current_revision, Some(BindingRevision::new(2)));
        assert_eq!(
            registry.resolve(&tenant("acme")).unwrap().revision,
            BindingRevision::new(2)
        );

        handle.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn a_failed_refresh_keeps_the_last_good_snapshot() {
        // The behaviour that matters most: an unreadable source must not empty
        // the registry and take every tenant down with it.
        let registry = Arc::new(TenantRuntimeRegistry::new());
        let source = Arc::new(InMemoryBindingSource::new(vec![binding_at("acme", 5)]));

        BindingRefresher::prime(&registry, source.as_ref()).await.unwrap();

        let handle = BindingRefresher::spawn(
            Arc::clone(&registry),
            Arc::clone(&source) as Arc<dyn BindingSource>,
            &config(),
        );

        source.fail_next();
        handle.refresh_now();

        // Give the background task a turn, then a second successful refresh to
        // prove the loop survived the failure.
        tokio::task::yield_now().await;
        handle.refresh_now();
        tokio::time::sleep(Duration::from_millis(50)).await;

        assert!(registry.is_primed());
        assert_eq!(
            registry.resolve(&tenant("acme")).unwrap().revision,
            BindingRevision::new(5)
        );

        handle.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn shutdown_stops_the_loop() {
        let registry = Arc::new(TenantRuntimeRegistry::new());
        let source = Arc::new(InMemoryBindingSource::empty()) as Arc<dyn BindingSource>;

        let handle = BindingRefresher::spawn(registry, source, &config());

        assert!(handle.shutdown().await.is_ok());
    }
}
