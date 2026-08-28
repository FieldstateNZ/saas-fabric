//! Liveness, for whatever is supervising the process.

use axum::routing::get;
use axum::{Json, Router};

/// The one route that does **not** require an operator.
///
/// A kubelet has no operator identity and cannot be given one, so requiring
/// authentication here would make every probe fail and the deployment would
/// never come up. That is safe because the endpoint says nothing: no client,
/// no configuration, no status. It reports that this process is running and
/// serving, which is exactly what a liveness probe asks.
///
/// Deliberately not a *readiness* probe. Readiness would have to answer "can
/// this process reach Git and Keycloak?", and answering it honestly means
/// calling both on every probe — putting probe traffic on the platform's
/// dependencies and coupling the deployment's health to theirs. The
/// reconciliation status the API already exposes is the better signal for that
/// question, and it is per client rather than for the process.
pub(super) fn routes() -> Router {
    Router::new().route(
        "/health",
        get(|| async { Json(serde_json::json!({"status": "ok"})) }),
    )
}
