//! `POST /api/clients/{clientId}/tombstone`

use axum::extract::State;
use axum::response::{IntoResponse, Response};
use axum::Json;
use http::header::ETAG;
use http::{HeaderMap, HeaderValue};

use crate::extraction::{BoundedJson, ClientPath};
use crate::models::{ClientResponse, TombstoneRequest};
use crate::state::ControlPlaneState;
use crate::{preconditions, ControlPlaneError, Operator};

/// Records a client's retirement, retaining its state (D01-14).
///
/// `If-Match` names the revision being retired, as every client write does.
/// This is intent only. Nothing is removed: the document, its placements, its
/// realm and its secrets all stay. Nothing is withdrawn either: the tenant
/// stays in the runtime documents until a separately reviewed withdrawal
/// exists, and the response's `lifecycle.withdrawal` says what publication
/// evidence shows rather than claiming it stopped being served.
pub(crate) async fn tombstone_client(
    operator: Operator,
    State(state): State<ControlPlaneState>,
    ClientPath(id): ClientPath,
    headers: HeaderMap,
    BoundedJson(request): BoundedJson<TombstoneRequest>,
) -> Result<Response, ControlPlaneError> {
    let expected = preconditions::required_revision(&headers)?;

    let stored = state
        .service
        .tombstone(&operator, &id, &request.reason, &expected)
        .await?;

    crate::converge::in_background(&state, &operator);

    let lifecycle = state.lifecycle(&stored).await;
    let mut response = Json(ClientResponse::from_stored(&stored, lifecycle)).into_response();
    if let Ok(tag) = HeaderValue::from_str(&preconditions::entity_tag(&stored.revision)) {
        response.headers_mut().insert(ETAG, tag);
    }

    Ok(response)
}
