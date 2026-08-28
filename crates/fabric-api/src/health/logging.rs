//! Structured log events for the probes.

use crate::health::connector_health::ConnectorOutcome;

/// Records every connector the sweep did not find healthy.
///
/// # Why this exists at all
///
/// The retry loop logs connectors that failed to *negotiate* at startup. A
/// connector that negotiated fine and went unhealthy later is not covered by
/// it, so without this line that failure would appear nowhere an operator
/// could reach without a token.
///
/// # Why `debug` and not `warn`
///
/// A `readinessProbe` runs every few seconds forever. At `warn` a single
/// connector down overnight is thousands of identical lines burying whatever
/// else needed attention; at `debug` the detail is one `RUST_LOG` change away
/// when somebody is actually looking, and costs nothing when they are not.
pub(super) fn connectors_swept(outcomes: &[ConnectorOutcome]) {
    for outcome in outcomes.iter().filter(|outcome| !outcome.health.is_healthy()) {
        tracing::debug!(
            event = "fabric.connector_unhealthy",
            connector = outcome.id,
            status = outcome.health.status(),
            reason = outcome
                .health
                .reason()
                .unwrap_or("no answer within the probe budget"),
            "connector did not report healthy during a readiness probe"
        );
    }
}
