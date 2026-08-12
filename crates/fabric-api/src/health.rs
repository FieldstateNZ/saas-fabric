//! Liveness and readiness.

use std::sync::Arc;

use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};
use fabric_connector::ConnectorRegistry;
use fabric_tenant_runtime::TenantRuntimeRegistry;
use http::StatusCode;

/// What the probes need to look at.
#[derive(Clone)]
pub struct HealthState {
    /// The tenant registry, for its primed flag.
    pub tenants: Arc<TenantRuntimeRegistry>,
    /// The connectors, to check reachability.
    pub connectors: ConnectorRegistry,
}

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
/// should fail when the tenant registry has not primed, because a replica with
/// no bindings can serve no tenant and would return 503 to everything it was
/// sent (§28).
pub fn health_routes(state: HealthState) -> Router {
    Router::new()
        .route("/health", get(liveness))
        .route("/ready", get(readiness))
        .with_state(state)
}

/// Liveness: the process is running and its event loop is turning.
async fn liveness() -> StatusCode {
    StatusCode::OK
}

/// Readiness: this replica can actually serve a request.
///
/// Connector health is checked as well as the registry. A connector that cannot
/// be reached makes every tenant bound to it unservable, and taking the replica
/// out of rotation is the right response while another replica may still be
/// fine.
async fn readiness(State(state): State<HealthState>) -> (StatusCode, Json<serde_json::Value>) {
    let primed = state.tenants.is_primed();

    let mut unhealthy = Vec::new();
    for connector in state.connectors.all() {
        if connector.health().await.is_err() {
            unhealthy.push(connector.id().to_string());
        }
    }

    let ready = primed && unhealthy.is_empty();

    let status = if ready {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };

    let body = serde_json::json!({
        "ready": ready,
        "tenant_registry_primed": primed,
        "tenants": state.tenants.len(),
        "unhealthy_connectors": unhealthy,
    });

    (status, Json(body))
}
