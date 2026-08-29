//! Turning a refusal into an HTTP response.

use axum::response::{IntoResponse, Response};
use axum::Json;
use http::header::RETRY_AFTER;
use http::{HeaderValue, StatusCode};

use crate::{logging, ControlPlaneError};

/// How long an operator is told to wait before repeating a request.
///
/// One coarse constant, matching the Data API's. It is a hint rather than a
/// contract, and it is attached only where retrying genuinely helps — which is
/// a property of the *error*, not of the status. Two errors answer 503, and
/// only one of them will be different in five seconds; a platform nobody has
/// connected to a repository yet will not connect itself.
const RETRY_AFTER_SECONDS: u32 = 5;

impl IntoResponse for ControlPlaneError {
    fn into_response(self) -> Response {
        let status = self.status();

        // Every 5xx is recorded here rather than at the throw site, so there
        // is exactly one place a masked internal detail is written down. The
        // 4xx that matters — a refused operator — is logged where it is
        // decided, because that is where the header name is known.
        if status.is_server_error() {
            logging::request_failed(self.code(), &self.to_string());
        }

        let body = serde_json::json!({
            "error": {
                "code": self.code(),
                "message": self.public_message(),
            }
        });

        let retryable = !matches!(self, ControlPlaneError::IntegrationNotConfigured);
        let mut response = (status, Json(body)).into_response();

        if status == StatusCode::SERVICE_UNAVAILABLE && retryable {
            response
                .headers_mut()
                .insert(RETRY_AFTER, HeaderValue::from(RETRY_AFTER_SECONDS));
        }

        response
    }
}
