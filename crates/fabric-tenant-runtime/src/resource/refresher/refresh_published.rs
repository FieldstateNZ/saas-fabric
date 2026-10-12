//! Keeping both registries current from the published documents, one whole
//! generation at a time.
//!
//! The prime, which applies the same rule once at start, is
//! `prime_published`.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use super::refresh_once::apply_loaded;
use super::{refresh_loop, RefreshHandle, ResourceRefresher};
use crate::publication::{PublishedState, PublishedStateSource, ReaderStatus, Serving};
use crate::resolution::PublishedGeneration;
use crate::resource::ResourceRegistry;
use crate::{logging, DataSource, RuntimeConfig, SourceError, TenantRuntimeBinding};

impl ResourceRefresher {
    /// Starts one background loop that, on every pass, reads the published
    /// documents and applies a complete generation, or nothing.
    ///
    /// An incomplete generation, a refused floor, an unreadable record, or a
    /// read that outlasts the interval each leave both registries serving the
    /// last complete generation. A complete one is written to both
    /// registries, then published to `generation` in one swap. Resolution
    /// reads only `generation`, so no request ever pairs one generation's
    /// tenant bindings with another's data sources -- including while the
    /// registries themselves hold one of each, between the two writes.
    #[must_use]
    pub(crate) fn spawn_published(
        generation: Arc<PublishedGeneration>,
        data_sources: Arc<ResourceRegistry<DataSource>>,
        tenants: Arc<ResourceRegistry<TenantRuntimeBinding>>,
        source: Arc<dyn PublishedStateSource>,
        status: Arc<ReaderStatus>,
        config: &RuntimeConfig,
    ) -> RefreshHandle {
        let description = source.describe();
        logging::refresher_started::<DataSource>(&description, config.refresh_interval_seconds);
        logging::refresher_started::<TenantRuntimeBinding>(&description, config.refresh_interval_seconds);
        let applied = Arc::new(AtomicU64::new(0));

        refresh_loop::spawn(
            config,
            move |timeout| {
                let (generation, data_sources, tenants) = (
                    Arc::clone(&generation),
                    Arc::clone(&data_sources),
                    Arc::clone(&tenants),
                );
                let (source, applied) = (Arc::clone(&source), Arc::clone(&applied));
                let status = Arc::clone(&status);
                async move {
                    let registries = (generation.as_ref(), data_sources.as_ref(), tenants.as_ref());
                    let reader = (source.as_ref(), status.as_ref());
                    refresh_published(registries, reader, &applied, timeout).await;
                }
            },
            move || {
                logging::refresher_stopped::<DataSource>(&description);
                logging::refresher_stopped::<TenantRuntimeBinding>(&description);
            },
        )
    }
}

type Registries<'a> = (
    &'a PublishedGeneration,
    &'a ResourceRegistry<DataSource>,
    &'a ResourceRegistry<TenantRuntimeBinding>,
);

async fn refresh_published(
    (generation, data_sources, tenants): Registries<'_>,
    (source, status): (&dyn PublishedStateSource, &ReaderStatus),
    applied: &AtomicU64,
    timeout: Duration,
) {
    let origin = source.describe();
    let Ok(loaded) = tokio::time::timeout(timeout, source.load()).await else {
        logging::generation_timed_out(&origin, timeout.as_secs());
        status.held(&SourceError::Unreadable {
            origin,
            cause: format!("the read outlasted {}s", timeout.as_secs()).into(),
        });
        return;
    };

    match loaded {
        Err(error) => {
            logging::generation_held(&origin, &error);
            status.held(&error);
        }
        Ok(PublishedState::Generation {
            generation: number,
            origin: read_from,
            data_sources: published_data_sources,
            tenants: published_tenants,
        }) => {
            apply_loaded(data_sources, &origin, Ok(published_data_sources));
            apply_loaded(tenants, &origin, Ok(published_tenants));
            generation.publish(tenants, data_sources);
            if applied.swap(number, Ordering::Relaxed) != number {
                logging::generation_applied(&origin, number);
            }
            status.applied(Serving::Generation {
                generation: number,
                origin: read_from,
            });
        }
        Ok(PublishedState::Unrecorded {
            data_sources: published_data_sources,
            tenants: published_tenants,
        }) => {
            apply_loaded(data_sources, &origin, published_data_sources);
            apply_loaded(tenants, &origin, published_tenants);
            generation.publish(tenants, data_sources);
            status.applied(Serving::Unrecorded);
        }
    }
}
