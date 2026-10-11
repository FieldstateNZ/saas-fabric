//! Wiring the runtime plane to the published documents, read as one
//! generation.

use std::sync::Arc;

use super::RuntimeHandles;
use crate::publication::PublishedStateSource;
use crate::resolution::PublishedGeneration;
use crate::resource::{ResourceRefresher, ResourceRegistry};
use crate::{RuntimeConfig, RuntimeResolver};

/// A published runtime, and the catalogue of the generation it primed.
pub struct PublishedRuntime {
    /// Resolves against the last complete generation applied.
    pub resolver: Arc<RuntimeResolver>,
    /// The refresher that keeps it current.
    pub handles: RuntimeHandles,
    /// The catalogue bytes read in the same read as the generation the
    /// runtime primed from, and checked against the same record. `None`
    /// when the source reads no catalogue, or the prime failed and the
    /// runtime started unprimed: then no generation is held to match.
    pub catalogue: Option<Vec<u8>>,
}

/// Validates configuration, primes both registries from one read of the
/// published documents, and starts the refresher that keeps them current a
/// whole generation at a time.
///
/// The resolver it returns reads only the last generation published to it,
/// tenant bindings and data sources together, never the registries
/// directly: see `PublishedGeneration`.
///
/// This is the production wiring: what [`build_runtime`](crate::build_runtime)
/// does with two independent sources, except that a publication the reader
/// finds incomplete -- in progress, interrupted, or projected one volume
/// ahead of the other -- is never applied in part. A process starting in
/// such a window finds no complete generation, which is a failed prime:
/// [`RuntimeConfig::fail_fast_on_prime`] decides whether that stops it or
/// starts it unprimed, answering 503 until a complete generation is read.
///
/// The catalogue is taken from the prime's own read
/// ([`PublishedStateSource::load_at_start`]), so it is the primed
/// generation's, never one published while the process started (G8).
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

    let primed =
        ResourceRefresher::prime_published(&generation, &data_sources, &tenants, source.as_ref()).await;
    let catalogue = match primed {
        Ok(catalogue) => catalogue,
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
            None
        }
    };

    let handles = RuntimeHandles {
        refresher: ResourceRefresher::spawn_published(
            Arc::clone(&generation),
            Arc::clone(&data_sources),
            Arc::clone(&tenants),
            source,
            config,
        ),
    };

    Ok(PublishedRuntime {
        resolver: Arc::new(RuntimeResolver::over_published(tenants, data_sources, generation)),
        handles,
        catalogue,
    })
}
