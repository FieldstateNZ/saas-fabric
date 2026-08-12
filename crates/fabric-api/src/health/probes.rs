//! The probe handlers.

use axum::extract::State;
use axum::Json;
use http::StatusCode;

use crate::health::HealthState;

/// Liveness: the process is running and its event loop is turning.
pub(super) async fn liveness() -> StatusCode {
    StatusCode::OK
}

/// Readiness: this replica can actually serve a request.
///
/// Three things must hold, and the response reports each separately so an
/// operator can see which one failed without reading logs:
///
/// - tenant bindings loaded,
/// - DataSources loaded,
/// - every connector reachable.
///
/// Connector health is included because a connector that cannot be reached
/// makes every tenant bound to it unservable, and taking this replica out of
/// rotation is the right response while another may still be fine.
pub(super) async fn readiness(State(state): State<HealthState>) -> (StatusCode, Json<serde_json::Value>) {
    let tenants_primed = state.runtime.tenants().is_primed();
    let data_sources_primed = state.runtime.data_sources().is_primed();

    let mut unhealthy = Vec::new();
    for connector in state.connectors.all() {
        if connector.health().await.is_err() {
            unhealthy.push(connector.id().to_string());
        }
    }

    let ready = tenants_primed && data_sources_primed && unhealthy.is_empty();

    let status = if ready {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };

    let body = serde_json::json!({
        "ready": ready,
        "tenants_primed": tenants_primed,
        "data_sources_primed": data_sources_primed,
        "tenants": state.runtime.tenants().len(),
        "data_sources": state.runtime.data_sources().len(),
        "unhealthy_connectors": unhealthy,
    });

    (status, Json(body))
}
