//! The probe handlers.

use axum::extract::State;
use axum::Json;
use http::StatusCode;

use crate::health::readiness_state::{is_degraded, is_ready};
use crate::health::HealthState;

/// Liveness: the process is running and its event loop is turning.
///
/// Checks nothing external — see [`crate::health`] for why. This must return
/// 200 even while every registry and every connector is down, or an
/// orchestrator restarting every replica turns a degraded system into a dead
/// one.
pub(super) async fn liveness() -> StatusCode {
    StatusCode::OK
}

/// Readiness: this replica can serve *some* traffic right now.
///
/// Gathers the two inputs the decision needs — registry priming, and a live
/// health check per connector — and renders them through
/// [`readiness_state`](crate::health::readiness_state), which owns the
/// decision itself. The body always lists every connector's individual
/// state, so an operator can see exactly what is degraded without reading
/// logs (§34).
pub(super) async fn readiness(State(state): State<HealthState>) -> (StatusCode, Json<serde_json::Value>) {
    let tenants_primed = state.runtime.tenants().is_primed();
    let data_sources_primed = state.runtime.data_sources().is_primed();

    let mut connectors = Vec::new();
    let mut healthy_connectors = 0usize;

    for connector in state.connectors.all() {
        match connector.health().await {
            Ok(()) => {
                healthy_connectors += 1;
                connectors.push(serde_json::json!({
                    "id": connector.id().as_str(),
                    "healthy": true,
                }));
            }
            Err(error) => {
                connectors.push(serde_json::json!({
                    "id": connector.id().as_str(),
                    "healthy": false,
                    "reason": error.to_string(),
                }));
            }
        }
    }

    let total_connectors = connectors.len();
    let ready = is_ready(
        tenants_primed,
        data_sources_primed,
        total_connectors,
        healthy_connectors,
    );
    let degraded = is_degraded(total_connectors, healthy_connectors);

    let status = if ready {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };

    let body = serde_json::json!({
        "ready": ready,
        "degraded": degraded,
        "tenants_primed": tenants_primed,
        "data_sources_primed": data_sources_primed,
        "tenants": state.runtime.tenants().len(),
        "data_sources": state.runtime.data_sources().len(),
        "connectors": connectors,
    });

    (status, Json(body))
}
