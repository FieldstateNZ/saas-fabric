//! Wiring for the tenant runtime domain.

use std::sync::Arc;

use crate::{BindingRefresher, BindingSource, RefreshHandle, TenantRuntimeConfig, TenantRuntimeRegistry};

/// Validates configuration, primes the registry, and starts the refresher.
///
/// Returns the registry and the handle controlling its refresher. The caller
/// keeps the handle: dropping it leaves the background task running with no way
/// to stop it, so the composition root should hold it until shutdown.
///
/// # Errors
///
/// - A message if the configuration is invalid.
/// - A message if the initial load fails **and**
///   [`TenantRuntimeConfig::fail_fast_on_prime`] is set. Otherwise a failed
///   prime is logged and the process starts unprimed, returning 503 until a
///   refresh succeeds.
pub async fn build_tenant_runtime(
    config: &TenantRuntimeConfig,
    source: Arc<dyn BindingSource>,
) -> Result<(Arc<TenantRuntimeRegistry>, RefreshHandle), String> {
    config.validate()?;

    let registry = Arc::new(TenantRuntimeRegistry::new());

    if let Err(error) = BindingRefresher::prime(&registry, source.as_ref()).await {
        if config.fail_fast_on_prime {
            return Err(format!(
                "could not load tenant bindings from {}: {error}",
                source.describe()
            ));
        }

        tracing::warn!(
            event = "tenant_runtime.prime_failed",
            source = source.describe(),
            reason = %error,
            "starting unprimed; the runtime will return 503 until a refresh succeeds"
        );
    }

    let handle = BindingRefresher::spawn(Arc::clone(&registry), source, config);

    Ok((registry, handle))
}
