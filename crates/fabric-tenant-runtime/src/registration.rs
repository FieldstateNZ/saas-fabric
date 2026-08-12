//! Wiring for the runtime plane.

#[cfg(test)]
mod registration_tests;

use std::sync::Arc;

use crate::resource::{RegistryResource, ResourceRefresher, ResourceRegistry, ResourceSource};
use crate::{DataSource, RefreshHandle, RuntimeConfig, RuntimeResolver, TenantRuntimeBinding};

/// The refresher handles the composition root must hold until shutdown.
///
/// Dropping either orphans its background task: the loop keeps polling with no
/// way to stop it.
pub struct RuntimeHandles {
    /// Controls the tenant binding refresher.
    pub tenants: RefreshHandle,

    /// Controls the DataSource refresher.
    pub data_sources: RefreshHandle,
}

impl RuntimeHandles {
    /// Stops both refreshers and waits for them.
    ///
    /// # Errors
    ///
    /// Returns the first join error if either background task panicked.
    pub async fn shutdown(self) -> Result<(), tokio::task::JoinError> {
        self.tenants.shutdown().await?;
        self.data_sources.shutdown().await
    }
}

/// Validates configuration, primes both registries, and starts their
/// refreshers.
///
/// The two sources are separate arguments because the two resources are
/// reconciled independently — a DataSource change should not require
/// republishing tenant bindings, and vice versa.
///
/// # Errors
///
/// - A message if the configuration is invalid.
/// - A message if an initial load fails **and**
///   [`RuntimeConfig::fail_fast_on_prime`] is set. Otherwise a failed prime is
///   logged and the process starts unprimed, returning 503 until a refresh
///   succeeds.
pub async fn build_runtime(
    config: &RuntimeConfig,
    tenant_source: Arc<dyn ResourceSource<TenantRuntimeBinding>>,
    data_source_source: Arc<dyn ResourceSource<DataSource>>,
) -> Result<(Arc<RuntimeResolver>, RuntimeHandles), String> {
    config.validate()?;

    // DataSources are primed first. A tenant binding referencing a DataSource
    // the registry has not loaded resolves to `MissingDataSource`, so loading
    // them in this order avoids a window of spurious failures at startup.
    let data_sources = prime(config, &data_source_source).await?;
    let tenants = prime(config, &tenant_source).await?;

    let handles = RuntimeHandles {
        data_sources: ResourceRefresher::spawn(Arc::clone(&data_sources), data_source_source, config),
        tenants: ResourceRefresher::spawn(Arc::clone(&tenants), tenant_source, config),
    };

    Ok((Arc::new(RuntimeResolver::new(tenants, data_sources)), handles))
}

/// Builds one registry and performs its initial load.
async fn prime<T: RegistryResource>(
    config: &RuntimeConfig,
    source: &Arc<dyn ResourceSource<T>>,
) -> Result<Arc<ResourceRegistry<T>>, String> {
    let registry = Arc::new(ResourceRegistry::<T>::new());

    if let Err(error) = ResourceRefresher::prime(&registry, source.as_ref()).await {
        if config.fail_fast_on_prime {
            return Err(format!(
                "could not load {} state from {}: {error}",
                T::KIND,
                source.describe()
            ));
        }

        tracing::warn!(
            event = "runtime.prime_failed",
            resource_kind = T::KIND,
            source = source.describe(),
            reason = %error,
            "starting unprimed; the runtime will return 503 until a refresh succeeds"
        );
    }

    Ok(registry)
}
