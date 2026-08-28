//! Tracing setup.

use tracing_subscriber::layer::SubscriberExt as _;
use tracing_subscriber::util::SubscriberInitExt as _;
use tracing_subscriber::EnvFilter;

/// Initialises structured logging.
///
/// JSON output, because these logs are read by machines first — and because
/// the control plane's audit records are log events (§24), which a
/// human-formatted line would make far harder to query reliably.
///
/// Per-domain levels come free from the crate layout — note the underscores:
///
/// ```text
/// RUST_LOG=info,fabric_control_plane=debug,fabric_keycloak=trace
/// ```
///
/// # Why this is not shared with the runtime host
///
/// It is fifteen near-identical lines that also exist in `fabric-api`. Sharing
/// them would mean either a dependency from the control plane onto a
/// runtime-plane crate — which the architecture check forbids, and rightly —
/// or a new shared crate whose only member is this function. Neither is worth
/// it; the duplication is visible, small, and has no invariant riding on the
/// two staying identical.
pub fn init() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    tracing_subscriber::registry()
        .with(filter)
        .with(tracing_subscriber::fmt::layer().json().with_current_span(true))
        .init();
}
