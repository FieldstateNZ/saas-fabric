//! `POST /api/clients/{clientId}/restoration`

use axum::extract::State;
use axum::response::{IntoResponse, Response};
use axum::Json;
use http::header::ETAG;
use http::{HeaderMap, HeaderValue};

use crate::extraction::{BoundedJson, ClientPath};
use crate::models::{ClientResponse, RestorationRequest};
use crate::state::ControlPlaneState;
use crate::{preconditions, ControlPlaneError, Operator};

/// Reverses a client's tombstone (D01-14).
///
/// `If-Match` names the tombstoned revision. The client becomes saved intent
/// again, with its tombstone kept in its history; the response's lifecycle
/// is `incomplete` until a pass applies its identity at the new revision.
/// Held with `503 lifecycle_continuity_unavailable`, before anything is
/// written, wherever no protected continuity store exists.
pub(crate) async fn restore_client(
    operator: Operator,
    State(state): State<ControlPlaneState>,
    ClientPath(id): ClientPath,
    headers: HeaderMap,
    BoundedJson(request): BoundedJson<RestorationRequest>,
) -> Result<Response, ControlPlaneError> {
    let expected = preconditions::required_revision(&headers)?;

    let facts = state.platform_facts().await;
    let stored = state
        .service
        .restore(
            &operator,
            &id,
            &request.reason,
            &expected,
            &facts,
            state.lifecycle_continuity.as_deref(),
        )
        .await?;

    crate::converge::in_background(&state, &operator);

    let lifecycle = state.lifecycle(&stored).await;
    let mut response = Json(ClientResponse::from_stored(&stored, lifecycle)).into_response();
    if let Ok(tag) = HeaderValue::from_str(&preconditions::entity_tag(&stored.revision)) {
        response.headers_mut().insert(ETAG, tag);
    }

    Ok(response)
}
