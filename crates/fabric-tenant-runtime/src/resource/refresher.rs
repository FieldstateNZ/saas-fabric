//! Keeping a registry current, in the background.
//!
//! In the 121-150 line band (docs/architecture/file-size-policy.md):
//! [`ResourceRefresher`] is one type, and its three entry points -- prime
//! once, refresh one registry, refresh two in order -- share one contract
//! about what a failed load leaves serving. The loop and the single reload
//! they share already live in `refresh_loop` and `refresh_once`.

mod refresh_handle;
mod refresh_loop;
mod refresh_once;
#[cfg(test)]
mod refresher_tests;

use std::sync::Arc;

use crate::resource::{RegistryResource, ResourceRegistry, ResourceSource};
use crate::{logging, RuntimeConfig, SourceError};

pub use refresh_handle::RefreshHandle;
use refresh_once::refresh_once;

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
    /// There is nothing special about this call. [`ResourceRegistry::apply_all`]
    /// refuses a first load that would install nothing, whoever makes it, so
    /// this path gets that protection by using the same method the refresh loop
    /// does rather than by remembering to pick a different one. All this adds is
    /// turning the refusal into a [`SourceError`] so `fail_fast_on_prime` has
    /// something to fire on.
    ///
    /// # Errors
    ///
    /// [`SourceError`] if the source could not be read, or if it read cleanly
    /// but published nothing usable. The caller decides whether that is fatal —
    /// see [`RuntimeConfig::fail_fast_on_prime`].
    pub async fn prime<T: RegistryResource>(
        registry: &ResourceRegistry<T>,
        source: &dyn ResourceSource<T>,
    ) -> Result<usize, SourceError> {
        let resources = source.load().await?;

        registry
            .apply_all(resources)
            .map_err(|refused| SourceError::NothingUsable {
                origin: source.describe(),
                count: refused.published,
                reason: refused.reason,
            })?;

        // Deliberately what the registry now holds, not what the source
        // returned. The merge drops resources that fail validation, and a
        // prime that reports a hundred tenants when three were rejected hides
        // the one number an operator needs.
        let count = registry.len();
        logging::primed::<T>(&source.describe(), count);

        Ok(count)
    }

    /// Starts a background loop that keeps one registry current.
    #[must_use]
    pub fn spawn<T: RegistryResource>(
        registry: Arc<ResourceRegistry<T>>,
        source: Arc<dyn ResourceSource<T>>,
        config: &RuntimeConfig,
    ) -> RefreshHandle {
        let description = source.describe();
        logging::refresher_started::<T>(&description, config.refresh_interval_seconds);

        refresh_loop::spawn(
            config,
            move || {
                let registry = Arc::clone(&registry);
                let source = Arc::clone(&source);
                async move { refresh_once(&registry, source.as_ref()).await }
            },
            move || logging::refresher_stopped::<T>(&description),
        )
    }

    /// Starts one background loop that, on every pass, reloads `first` and
    /// only then reloads `then`.
    ///
    /// # Why one loop rather than two
    ///
    /// A tenant binding naming a DataSource the registry has not loaded
    /// resolves to `MissingDataSource`, a 500. The publisher writes data
    /// sources before tenants for exactly that reason, and two independent
    /// loops threw the order away: either could fire first. One loop that
    /// reads `first` before `then` carries the publisher's order to the
    /// reader whenever both documents are already on disk, which is the
    /// filesystem layout. It cannot help when `then` is newer on disk than
    /// `first` -- a kubelet projecting one `ConfigMap` volume ahead of
    /// another -- because no order of reads makes a file arrive sooner.
    ///
    /// A failed load of `first` does not skip `then`. Each keeps its own last
    /// good snapshot, exactly as two loops did, so an unreadable data-sources
    /// document never holds back a tenant change such as a deprovisioning.
    #[must_use]
    pub fn spawn_in_order<A: RegistryResource, B: RegistryResource>(
        first: Arc<ResourceRegistry<A>>,
        first_source: Arc<dyn ResourceSource<A>>,
        then: Arc<ResourceRegistry<B>>,
        then_source: Arc<dyn ResourceSource<B>>,
        config: &RuntimeConfig,
    ) -> RefreshHandle {
        let first_description = first_source.describe();
        let then_description = then_source.describe();
        logging::refresher_started::<A>(&first_description, config.refresh_interval_seconds);
        logging::refresher_started::<B>(&then_description, config.refresh_interval_seconds);

        refresh_loop::spawn(
            config,
            move || {
                let (first, first_source) = (Arc::clone(&first), Arc::clone(&first_source));
                let (then, then_source) = (Arc::clone(&then), Arc::clone(&then_source));
                async move {
                    refresh_once(&first, first_source.as_ref()).await;
                    refresh_once(&then, then_source.as_ref()).await;
                }
            },
            move || {
                logging::refresher_stopped::<A>(&first_description);
                logging::refresher_stopped::<B>(&then_description);
            },
        )
    }
}
