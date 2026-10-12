//! Wiring the runtime plane to the published documents, read as one
//! generation.

use std::sync::Arc;

use super::RuntimeHandles;
use crate::publication::{PublishedStateSource, ReaderStatus};
use crate::resolution::PublishedGeneration;
use crate::resource::{ResourceRefresher, ResourceRegistry};
use crate::{RuntimeConfig, RuntimeResolver};

/// A published runtime.
pub struct PublishedRuntime {
    /// Pins the last complete view installed, catalogue included.
    pub resolver: Arc<RuntimeResolver>,
    /// The refresher that keeps it current.
    pub handles: RuntimeHandles,
    /// What it serves and why, and its last-known-good copy, for readiness.
    pub status: Arc<ReaderStatus>,
}

/// Validates configuration, installs the first view from one read of the
/// published documents, and starts the refresher that keeps it current a
/// whole generation at a time.
///
/// The resolver it returns reads only the last view installed: catalogue,
/// tenant bindings and data sources together, never the registries
/// directly. See `PublishedGeneration`.
///
/// This is the production wiring: what [`build_runtime`](crate::build_runtime)
/// does with two independent sources, except that a publication the reader
/// finds incomplete -- in progress, interrupted, or projected one volume
/// ahead of the other -- is never applied in part. A process starting in
/// such a window finds no complete generation, which is a failed prime:
/// [`RuntimeConfig::fail_fast_on_prime`] decides whether that stops it or
/// starts it unprimed, answering 503 until a complete generation is read.
///
/// There is no catalogue apart from the view's (G8). A runtime that starts
/// unprimed serves no catalogue at all, so a host answers every request as
/// unavailable until the first complete view installs; it never falls back
/// on a catalogue read on its own.
///
/// # Errors
///
/// A message if the configuration is invalid, or if the prime fails and
/// `fail_fast_on_prime` is set.
pub async fn build_published_runtime(
    config: &RuntimeConfig,
    source: Arc<dyn PublishedStateSource>,
) -> Result<PublishedRuntime, String> {
    config.validate()?;

    let data_sources = Arc::new(ResourceRegistry::new());
    let tenants = Arc::new(ResourceRegistry::new());
    let generation = Arc::new(PublishedGeneration::default());
    let status = Arc::new(ReaderStatus::over(Arc::clone(&source)));

    let registries = (generation.as_ref(), data_sources.as_ref(), tenants.as_ref());
    match ResourceRefresher::prime_published(registries, source.as_ref(), &status).await {
        Ok(()) => {}
        Err(error) if config.fail_fast_on_prime => {
            return Err(format!(
                "could not load the published runtime state from {}: {error}",
                source.describe()
            ));
        }
        Err(error) => {
            tracing::warn!(
                event = "runtime.prime_failed",
                source = source.describe(),
                reason = %error,
                "starting unprimed; the runtime will return 503 until a complete generation is read"
            );
        }
    }

    let handles = RuntimeHandles {
        refresher: ResourceRefresher::spawn_published(
            Arc::clone(&generation),
            Arc::clone(&data_sources),
            Arc::clone(&tenants),
            source,
            Arc::clone(&status),
            config,
        ),
    };

    Ok(PublishedRuntime {
        resolver: Arc::new(RuntimeResolver::over_published(tenants, data_sources, generation)),
        handles,
        status,
    })
}
