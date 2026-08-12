//! The probe routes.

use axum::routing::get;
use axum::Router;

use crate::health::probes::{liveness, readiness};
use crate::health::HealthState;

/// Builds the probe routes.
///
/// See [`crate::health`] for the liveness/readiness distinction these two
/// routes exist to keep rigorous.
pub fn health_routes(state: HealthState) -> Router {
    Router::new()
        .route("/health", get(liveness))
        .route("/ready", get(readiness))
        .with_state(state)
}
