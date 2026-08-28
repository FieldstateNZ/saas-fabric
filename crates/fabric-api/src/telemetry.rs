//! Tracing setup.

use tracing_subscriber::layer::SubscriberExt as _;
use tracing_subscriber::util::SubscriberInitExt as _;
use tracing_subscriber::EnvFilter;

/// Initialises structured logging.
///
/// JSON output, because these logs are read by machines first. Per-domain
/// levels come free from the crate layout — the filter targets are crate paths:
///
/// ```text
/// RUST_LOG=info,fabric_tenant_runtime=debug,fabric_connector_ndc=trace
/// ```
///
/// Note the underscores. The crate is `fabric-tenant-runtime`; the filter
/// target is `fabric_tenant_runtime`. This catches everyone once.
pub fn init() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    tracing_subscriber::registry()
        .with(filter)
        .with(tracing_subscriber::fmt::layer().json().with_current_span(true))
        .init();
}
