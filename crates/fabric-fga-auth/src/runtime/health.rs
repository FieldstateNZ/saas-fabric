//! Liveness and readiness, which answer different questions.

use axum::extract::State;
use axum::http::StatusCode;

use super::RuntimeSurface;

/// `GET /health/live` — is this process itself working?
///
/// **Only the front.** It must not depend on the identity provider or the
/// authorization service, because a failing liveness probe restarts the
/// container: making it depend on a neighbour turns that neighbour's outage
/// into a restart loop here, which helps nothing and destroys the verifier's
/// cached keys — the very thing that would have kept this process useful.
pub(super) async fn live() -> StatusCode {
    StatusCode::OK
}

/// `GET /health/ready` — should this process be sent traffic?
///
/// Includes the embedded authorization service, because a front that cannot
/// reach it cannot answer anything and should leave rotation.
///
/// It deliberately does **not** require a fresh key fetch. The verifier is
/// built to keep working through an identity-provider outage on cached
/// known-good keys, and a readiness probe that insisted otherwise would take
/// the front out of service for exactly the condition it was designed to
/// survive.
pub(super) async fn ready(State(surface): State<RuntimeSurface>) -> StatusCode {
    if surface.decisions.reachable().await {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    }
}
