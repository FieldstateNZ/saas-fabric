//! `GET /api/clients/{clientId}/identity`

use axum::extract::State;
use axum::response::{IntoResponse, Response};
use axum::Json;
use http::header::ETAG;
use http::{HeaderValue, StatusCode};

use crate::extraction::ClientPath;
use crate::models::IdentityResponse;
use crate::state::ControlPlaneState;
use crate::{preconditions, ControlPlaneError, Operator};

/// A client's identity configuration, and where reconciliation stands.
///
/// The revision is returned twice — as the `ETag` header and as a field — and
/// both are deliberate. The header is what an HTTP client naturally echoes
/// back in `If-Match`; the field is what a browser can always read, since
/// `ETag` is not among the response headers exposed cross-origin by default.
/// A UI that could not obtain the revision could not make a safe write at all,
/// so it is worth saying twice.
pub(crate) async fn get_identity(
    State(state): State<ControlPlaneState>,
    ClientPath(client_id): ClientPath,
    _operator: Operator,
) -> Result<Response, ControlPlaneError> {
    let stored = state.service.get(&client_id).await?;
    let reconciliation = state.service.reconciliation(&stored);

    let body = IdentityResponse::new(
        &stored.document.client().identity,
        stored.revision.clone(),
        reconciliation,
    );

    let mut response = (StatusCode::OK, Json(body)).into_response();

    // The revision's character set is checked at parse time precisely so that
    // this cannot fail; if it somehow did, the body still carries the
    // revision, so the response is degraded rather than broken.
    if let Ok(tag) = HeaderValue::from_str(&preconditions::entity_tag(&stored.revision)) {
        response.headers_mut().insert(ETAG, tag);
    }

    Ok(response)
}
