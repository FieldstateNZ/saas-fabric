//! Joining the domains into the one surface the process serves.
//!
//! Extracted from [`build`](super::build) and made `pub` for a specific
//! reason: `tests/composed_surface.rs` exists to assert the shape of the
//! router the process listens on, and it used to do that against its own
//! hand-assembled copy. The copy drifted — no timeout scope, no probe routes,
//! no tracing — and the gap is what let a `/ready` with no bound on it through
//! review. A test that rebuilds the thing it is checking is checking itself.

use std::time::Duration;

use axum::Router;
use http::StatusCode;
use tower_http::timeout::TimeoutLayer;
use tower_http::trace::TraceLayer;

/// Joins the Data API and the probe routes into the served router.
///
/// # The timeout scope is the point
///
/// `request_timeout` wraps the Data API **only**, and that placement is a
/// decision rather than an oversight. A probe is not a request with a budget:
/// a `/ready` that answered 504 would be recorded by a kubelet as a failure
/// and the replica pulled, which is precisely the outcome the readiness policy
/// exists to avoid. The probe bounds itself instead — see
/// the health module's `connector_sweep` — and produces a
/// verdict either way.
///
/// # Merged, not nested
///
/// The Data API router carries its whole path itself — `/v1/data/...`, see
/// `fabric_data_api::API_PREFIX` — so it is merged. Nesting a `/data` prefix
/// here on top of a router that already knows its version would produce
/// `/data/v1/...`, with the version buried a segment deep.
pub fn compose(data: Router, health: Router, request_timeout: Duration) -> Router {
    Router::new()
        .merge(data.layer(TimeoutLayer::with_status_code(
            StatusCode::GATEWAY_TIMEOUT,
            request_timeout,
        )))
        .merge(health)
        .layer(TraceLayer::new_for_http())
}
