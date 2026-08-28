//! Joining the control-plane API and the probe route into one surface.

use std::time::Duration;

use axum::http::StatusCode;
use axum::Router;
use tower_http::timeout::TimeoutLayer;
use tower_http::trace::TraceLayer;

use crate::config::ControlPlaneAppConfig;
use crate::startup::health;

/// Composes the served router.
///
/// # The timeout wraps the API and not the probe
///
/// The same placement decision the runtime host makes, for the same reason: a
/// probe that answered `504` would be recorded as a failure and the replica
/// pulled, which is precisely the outcome a liveness probe exists to avoid.
/// The probe bounds itself by doing nothing.
pub(super) fn compose(api: Router, config: &ControlPlaneAppConfig) -> Router {
    let timeout = Duration::from_secs(config.request_timeout_seconds);

    Router::new()
        .merge(api.layer(TimeoutLayer::with_status_code(
            StatusCode::GATEWAY_TIMEOUT,
            timeout,
        )))
        .merge(health::routes())
        .layer(TraceLayer::new_for_http())
}
