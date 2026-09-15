//! `POST /api/clients`

use axum::extract::State;
use axum::response::{IntoResponse, Response};
use axum::Json;
use fabric_client_model::catalogue::CreateClientRequest;
use http::header::ETAG;
use http::{HeaderValue, StatusCode};

use crate::extraction::BoundedJson;
use crate::models::ClientProductResponse;
use crate::state::ControlPlaneState;
use crate::{preconditions, ControlPlaneError, Operator};

/// Creates a client, resolving its requested applications against the
/// catalogue's published releases at creation time.
///
/// # Why creation belongs beside product configuration, not identity
///
/// A client cannot exist with no product configuration — the applications it
/// is entitled to are part of what "creating a client" means here, unlike
/// identity, which starts empty and is configured afterwards through its own
/// endpoint. So this writes one document with both in it, in one commit.
pub(crate) async fn create_client(
    operator: Operator,
    State(state): State<ControlPlaneState>,
    BoundedJson(request): BoundedJson<CreateClientRequest>,
) -> Result<Response, ControlPlaneError> {
    let stored = state.service.create_client(&operator, request).await?;
    let reconciliation = state.service.reconciliation(&stored);
    let body = ClientProductResponse::from_stored(&stored, reconciliation)?;

    // After the write, and after the response is decided: convergence is a
    // separate event that fails independently, and this one borrows an
    // authority that belongs to the person who just made the request.
    crate::converge::in_background(&state, &operator);

    let mut response = (StatusCode::CREATED, Json(body)).into_response();

    if let Ok(tag) = HeaderValue::from_str(&preconditions::entity_tag(&stored.revision)) {
        response.headers_mut().insert(ETAG, tag);
    }

    Ok(response)
}
