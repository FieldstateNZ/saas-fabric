//! The probe handlers.

use axum::extract::State;
use axum::Json;
use http::{HeaderMap, StatusCode};
use serde_json::Value;

use crate::health::detail_access::may_see_detail;
use crate::health::readiness_facts::{ConnectorFacts, RegistryFacts};
use crate::health::readiness_state::{is_degraded, is_ready};
use crate::health::{connector_sweep, logging, readiness_body, HealthState};

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
/// Gathers the facts the decision needs — what the two registries hold, and a
/// bounded concurrent health sweep across the connectors — and renders them
/// through [`readiness_state`](crate::health::readiness_state), which owns the
/// decision itself.
///
/// The status code is the whole answer for an orchestrator and is the same for
/// every caller. What differs is the body: the per-connector detail names
/// physical infrastructure, so it is shown only to a caller holding the
/// administrator role (see
/// [`detail_access`](crate::health::detail_access)).
pub(super) async fn readiness(
    State(state): State<HealthState>,
    headers: HeaderMap,
) -> (StatusCode, Json<Value>) {
    let outcomes = connector_sweep::sweep(&state.connectors).await;

    let tenants = RegistryFacts {
        primed: state.runtime.tenants().is_primed(),
        count: state.runtime.tenants().len(),
    };
    let data_sources = RegistryFacts {
        primed: state.runtime.data_sources().is_primed(),
        count: state.runtime.data_sources().len(),
    };
    let connectors = ConnectorFacts::from(outcomes.as_slice());

    let ready = is_ready(&tenants, &data_sources, &connectors);
    let degraded = is_degraded(&connectors);

    logging::connectors_swept(&outcomes);

    let status = if ready {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };

    let body = if may_see_detail(&state, &headers) {
        readiness_body::detailed(ready, degraded, &tenants, &data_sources, &outcomes)
    } else {
        readiness_body::minimal(ready)
    };

    (status, Json(body))
}
