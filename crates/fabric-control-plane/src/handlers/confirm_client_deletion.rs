//! `POST /api/clients/{clientId}/deletion-confirmation`

use axum::extract::State;
use axum::response::{IntoResponse, Response};
use axum::Json;
use http::header::ETAG;
use http::{HeaderMap, HeaderValue};

use crate::extraction::{BoundedJson, ClientPath};
use crate::models::{ClientResponse, DeletionConfirmationRequest};
use crate::state::ControlPlaneState;
use crate::{preconditions, ControlPlaneError, Operator};

/// Confirms deletion of a tombstoned client (D01-14).
///
/// A separate write from the tombstone, at the revision the tombstone
/// produced, naming the client again in the body. It records intent only:
/// nothing is deleted, and no retention period is applied, because neither
/// is decided.
pub(crate) async fn confirm_client_deletion(
    operator: Operator,
    State(state): State<ControlPlaneState>,
    ClientPath(id): ClientPath,
    headers: HeaderMap,
    BoundedJson(request): BoundedJson<DeletionConfirmationRequest>,
) -> Result<Response, ControlPlaneError> {
    let expected = preconditions::required_revision(&headers)?;

    let stored = state
        .service
        .confirm_deletion(&operator, &id, &request.confirm_client_id, &expected)
        .await?;

    crate::converge::in_background(&state, &operator);

    let lifecycle = state.lifecycle(&stored).await;
    let mut response = Json(ClientResponse::from_stored(&stored, lifecycle)).into_response();
    if let Ok(tag) = HeaderValue::from_str(&preconditions::entity_tag(&stored.revision)) {
        response.headers_mut().insert(ETAG, tag);
    }

    Ok(response)
}
