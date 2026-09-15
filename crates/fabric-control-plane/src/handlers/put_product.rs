//! `PUT /api/clients/{clientId}/product`

use axum::extract::State;
use axum::response::{IntoResponse, Response};
use axum::Json;
use fabric_client_model::catalogue::ClientProductRequest;
use http::header::ETAG;
use http::{HeaderMap, HeaderValue};

use crate::extraction::{BoundedJson, ClientPath};
use crate::models::ClientProductResponse;
use crate::state::ControlPlaneState;
use crate::{preconditions, ControlPlaneError, Operator};

/// Replaces a client's editable product configuration.
///
/// # Why removing an assigned application is refused
///
/// This endpoint has no way to deprovision what an application already did
/// with that client's data, so it refuses to make a document claim the
/// application is gone while whatever it created is still there. See
/// `ClientService::set_product`.
pub(crate) async fn put_product(
    operator: Operator,
    State(state): State<ControlPlaneState>,
    ClientPath(id): ClientPath,
    headers: HeaderMap,
    BoundedJson(request): BoundedJson<ClientProductRequest>,
) -> Result<Response, ControlPlaneError> {
    let expected = preconditions::required_revision(&headers)?;

    let stored = state
        .service
        .set_product(&operator, &id, request, &expected)
        .await?;
    let reconciliation = state.service.reconciliation(&stored);
    let body = ClientProductResponse::from_stored(&stored, reconciliation)?;

    // Same reasoning as `create_client`: convergence follows the answered
    // write, and runs with this operator's own authority (ADR 0008, ADR
    // 0012).
    crate::converge::in_background(&state, &operator);

    let mut response = Json(body).into_response();

    if let Ok(tag) = HeaderValue::from_str(&preconditions::entity_tag(&stored.revision)) {
        response.headers_mut().insert(ETAG, tag);
    }

    Ok(response)
}
