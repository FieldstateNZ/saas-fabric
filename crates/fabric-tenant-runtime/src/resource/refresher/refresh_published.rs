//! Keeping both registries current from the published documents, one whole
//! generation at a time.
//!
//! In the 121-150 line band (docs/architecture/file-size-policy.md): the
//! prime and the refresh pass share one rule -- a complete generation is
//! applied whole, data sources first, or not at all -- and keeping them
//! together keeps the two from drifting apart.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use super::refresh_once::apply_loaded;
use super::{refresh_loop, RefreshHandle, ResourceRefresher};
use crate::publication::{PublishedState, PublishedStateSource};
use crate::resource::{RegistryResource, ResourceRegistry};
use crate::{logging, DataSource, RuntimeConfig, SourceError, TenantRuntimeBinding};

impl ResourceRefresher {
    /// Loads the published documents once, so both registries can serve:
    /// a complete generation is applied whole, data sources first; an
    /// unrecorded set is applied a document at a time, as before the record
    /// existed.
    ///
    /// # Errors
    ///
    /// [`SourceError`] if no generation could be applied, or if a document
    /// read cleanly but nothing in it could be served. Of a complete
    /// generation, the tenants are never applied once its data sources were
    /// refused: that would prime one registry from a generation the other
    /// does not hold.
    pub async fn prime_published(
        data_sources: &ResourceRegistry<DataSource>,
        tenants: &ResourceRegistry<TenantRuntimeBinding>,
        source: &dyn PublishedStateSource,
    ) -> Result<(), SourceError> {
        let origin = source.describe();
        match source.load().await? {
            PublishedState::Generation {
                generation,
                data_sources: published_data_sources,
                tenants: published_tenants,
            } => {
                prime_with(data_sources, &origin, Ok(published_data_sources))?;
                prime_with(tenants, &origin, Ok(published_tenants))?;
                logging::generation_applied(&origin, generation);
                Ok(())
            }
            PublishedState::Unrecorded {
                data_sources: published_data_sources,
                tenants: published_tenants,
            } => {
                let data_sources = prime_with(data_sources, &origin, published_data_sources);
                let tenants = prime_with(tenants, &origin, published_tenants);
                data_sources.and(tenants)
            }
        }
    }

    /// Starts one background loop that, on every pass, reads the published
    /// documents and applies a complete generation, or nothing.
    ///
    /// An incomplete generation, a refused floor, an unreadable record, or a
    /// read that outlasts the interval each leave both registries serving the
    /// last complete generation. Within a complete one, data sources are
    /// applied before tenants: the two registries swap one after the other,
    /// so a request resolving between the two swaps sees the new data
    /// sources with the previous tenants -- for an instant, and never a
    /// tenant bound to a data source the generation it came from lacked.
    #[must_use]
    pub fn spawn_published(
        data_sources: Arc<ResourceRegistry<DataSource>>,
        tenants: Arc<ResourceRegistry<TenantRuntimeBinding>>,
        source: Arc<dyn PublishedStateSource>,
        config: &RuntimeConfig,
    ) -> RefreshHandle {
        let description = source.describe();
        logging::refresher_started::<DataSource>(&description, config.refresh_interval_seconds);
        logging::refresher_started::<TenantRuntimeBinding>(&description, config.refresh_interval_seconds);
        let applied = Arc::new(AtomicU64::new(0));

        refresh_loop::spawn(
            config,
            move |timeout| {
                let (data_sources, tenants) = (Arc::clone(&data_sources), Arc::clone(&tenants));
                let (source, applied) = (Arc::clone(&source), Arc::clone(&applied));
                async move {
                    refresh_published(&data_sources, &tenants, source.as_ref(), &applied, timeout).await;
                }
            },
            move || {
                logging::refresher_stopped::<DataSource>(&description);
                logging::refresher_stopped::<TenantRuntimeBinding>(&description);
            },
        )
    }
}

async fn refresh_published(
    data_sources: &ResourceRegistry<DataSource>,
    tenants: &ResourceRegistry<TenantRuntimeBinding>,
    source: &dyn PublishedStateSource,
    applied: &AtomicU64,
    timeout: Duration,
) {
    let origin = source.describe();
    let Ok(loaded) = tokio::time::timeout(timeout, source.load()).await else {
        logging::generation_timed_out(&origin, timeout.as_secs());
        return;
    };

    match loaded {
        Err(error) => logging::generation_held(&origin, &error),
        Ok(PublishedState::Generation {
            generation,
            data_sources: published_data_sources,
            tenants: published_tenants,
        }) => {
            apply_loaded(data_sources, &origin, Ok(published_data_sources));
            apply_loaded(tenants, &origin, Ok(published_tenants));
            if applied.swap(generation, Ordering::Relaxed) != generation {
                logging::generation_applied(&origin, generation);
            }
        }
        Ok(PublishedState::Unrecorded {
            data_sources: published_data_sources,
            tenants: published_tenants,
        }) => {
            apply_loaded(data_sources, &origin, published_data_sources);
            apply_loaded(tenants, &origin, published_tenants);
        }
    }
}

fn prime_with<T: RegistryResource>(
    registry: &ResourceRegistry<T>,
    origin: &str,
    loaded: Result<Vec<T>, SourceError>,
) -> Result<(), SourceError> {
    registry
        .apply_all(loaded?)
        .map_err(|refused| SourceError::NothingUsable {
            origin: origin.to_owned(),
            count: refused.published,
            reason: refused.reason,
        })?;
    logging::primed::<T>(origin, registry.len());
    Ok(())
}
