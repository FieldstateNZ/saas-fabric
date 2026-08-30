//! Assembling the runtime surface from configuration.

use std::sync::Arc;

use fabric_core::SystemClock;
use fabric_fga_auth::{
    Check, Decisions, HttpKeySource, KeyCache, OpenFgaDecisions, Registry, RuntimeSurface, Verifier,
};

use crate::config::AppConfig;

/// Builds the router this host serves.
///
/// # Errors
///
/// Returns a message if the registry is not usable — an empty one, a
/// duplicated issuer, or a registration missing something it cannot work
/// without. All of them are fatal here rather than per request.
pub fn build(config: &AppConfig) -> Result<axum::Router, String> {
    let registry =
        Registry::build(config.issuers.clone()).map_err(|error| format!("issuer registry: {error}"))?;

    tracing::info!(
        event = "authorization_front.registry",
        issuers = registry.len(),
        "trusting the configured issuers"
    );

    let keys = Arc::new(KeyCache::new(
        Arc::new(HttpKeySource::new()?),
        Arc::new(SystemClock),
    ));

    let decisions = Arc::new(
        OpenFgaDecisions::on_loopback(config.embedded.port)
            .map_err(|error| format!("authorization service client: {error}"))?,
    );

    let surface = RuntimeSurface::new(
        Arc::new(Verifier::new(registry, keys)),
        Arc::new(Check::new(Arc::clone(&decisions) as Arc<dyn Decisions>)),
        decisions as Arc<dyn Decisions>,
    );

    Ok(surface.router())
}
