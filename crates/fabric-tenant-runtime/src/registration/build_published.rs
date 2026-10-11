//! Wiring the runtime plane to the published documents, read as one
//! generation.

use std::sync::Arc;

use super::RuntimeHandles;
use crate::publication::PublishedStateSource;
use crate::resource::{ResourceRefresher, ResourceRegistry};
use crate::{RuntimeConfig, RuntimeResolver};

/// Validates configuration, primes both registries from one read of the
/// published documents, and starts the refresher that keeps them current a
/// whole generation at a time.
///
/// This is the production wiring: what [`build_runtime`](crate::build_runtime)
/// does with two independent sources, except that a publication the reader
/// finds incomplete -- in progress, interrupted, or projected one volume
/// ahead of the other -- is never applied in part. A process starting in
/// such a window finds no complete generation, which is a failed prime:
/// [`RuntimeConfig::fail_fast_on_prime`] decides whether that stops it or
/// starts it unprimed, answering 503 until a complete generation is read.
///
/// # Errors
///
/// A message if the configuration is invalid, or if the prime fails and
/// `fail_fast_on_prime` is set.
pub async fn build_published_runtime(
    config: &RuntimeConfig,
    source: Arc<dyn PublishedStateSource>,
) -> Result<(Arc<RuntimeResolver>, RuntimeHandles), String> {
    config.validate()?;

    let data_sources = Arc::new(ResourceRegistry::new());
    let tenants = Arc::new(ResourceRegistry::new());

    if let Err(error) = ResourceRefresher::prime_published(&data_sources, &tenants, source.as_ref()).await {
        if config.fail_fast_on_prime {
            return Err(format!(
                "could not load the published runtime state from {}: {error}",
                source.describe()
            ));
        }
        tracing::warn!(
            event = "runtime.prime_failed",
            source = source.describe(),
            reason = %error,
            "starting unprimed; the runtime will return 503 until a complete generation is read"
        );
    }

    let handles = RuntimeHandles {
        refresher: ResourceRefresher::spawn_published(
            Arc::clone(&data_sources),
            Arc::clone(&tenants),
            source,
            config,
        ),
    };

    Ok((Arc::new(RuntimeResolver::new(tenants, data_sources)), handles))
}
