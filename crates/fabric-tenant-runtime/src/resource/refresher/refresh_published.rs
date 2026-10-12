//! Keeping the published runtime current, one whole view at a time.
//!
//! The prime, which applies the same rule once at start, is
//! `prime_published`.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use super::install::{install, Registries};
use super::prime_published::settle;
use super::{refresh_loop, RefreshHandle, ResourceRefresher};
use crate::publication::{PublishedStateSource, ReaderStatus};
use crate::resolution::PublishedGeneration;
use crate::resource::ResourceRegistry;
use crate::{logging, DataSource, RuntimeConfig, SourceError, TenantRuntimeBinding};

impl ResourceRefresher {
    /// Starts one background loop that, on every pass, reads the published
    /// documents and installs a complete view, or nothing.
    ///
    /// An incomplete generation (the catalogue included), a catalogue the
    /// host cannot decode, a refused floor, an unreadable record, a lost
    /// record after a recorded one, a set a registry would not take whole,
    /// or a read that outlasts the interval each leave the whole installed
    /// view serving: catalogue, tenants, data sources and stamp. A complete
    /// one is written to both registries, then installed with its catalogue
    /// in one swap. Requests read only that view, so none pairs one
    /// generation's catalogue or bindings with another's (G1b, G8).
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

async fn refresh_published(
    registries: Registries<'_>,
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

    match loaded.and_then(|state| install(state, registries, &origin)) {
        Err(error) => {
            logging::generation_held(&origin, &error);
            status.held(&error);
        }
        Ok(installed) => {
            if let Some((generation, record_sha256)) = installed.recorded {
                if applied.swap(generation, Ordering::Relaxed) == generation {
                    source.installed(generation, &record_sha256).await;
                    status.applied(installed.serving);
                    return;
                }
            }
            settle(installed, source, status).await;
        }
    }
}
