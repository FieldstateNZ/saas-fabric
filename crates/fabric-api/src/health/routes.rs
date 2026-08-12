//! The probe routes.

use axum::routing::get;
use axum::Router;

use crate::health::probes::{liveness, readiness};
use crate::health::HealthState;

/// Builds the probe routes.
///
/// # Why these are two different questions
///
/// **Liveness** asks whether the process is wedged. It must not depend on
/// anything external — a liveness probe that fails when a database is down
/// causes the orchestrator to restart every replica during an outage, turning a
/// degraded system into a dead one.
///
/// **Readiness** asks whether this replica can serve traffic. It genuinely
/// should fail when a registry has not primed, because a replica with no
/// bindings or no DataSources can serve no tenant and would 503 everything it
/// was sent (§28).
pub fn health_routes(state: HealthState) -> Router {
    Router::new()
        .route("/health", get(liveness))
        .route("/ready", get(readiness))
        .with_state(state)
}
