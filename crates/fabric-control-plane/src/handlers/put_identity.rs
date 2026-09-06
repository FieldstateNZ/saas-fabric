//! `PUT /api/clients/{clientId}/identity`

use axum::extract::State;
use axum::response::{IntoResponse, Response};
use axum::Json;
use fabric_client_model::IdentityConfiguration;
use http::header::ETAG;
use http::{HeaderMap, HeaderValue, StatusCode};

use crate::extraction::{BoundedJson, ClientPath};
use crate::models::{IdentityRequest, IdentityResponse};
use crate::state::ControlPlaneState;
use crate::{preconditions, ControlPlaneError, Operator};

/// Replaces a client's identity configuration.
///
/// # What this endpoint does not do
///
/// It does not call Keycloak *before answering*. It writes a document to the
/// desired-state repository and records that reconciliation is pending. The
/// response says `pending` for exactly that reason: at the moment it is
/// written, the identity provider provably has not been changed, and reporting
/// `applied` would be a claim the platform cannot support (ADR 0008).
///
/// Convergence is then attempted in the background **with this operator's
/// authority**, because the platform has none of its own (ADR 0012). Under a
/// posture that cannot lend one, nothing is attempted and the client stays
/// `pending` — which is true.
///
/// # `200`, not `202`
///
/// `202 Accepted` would be defensible — something asynchronous does follow —
/// but the thing this endpoint is responsible for has completed: desired state
/// is durably written, and the response body carries the new revision. `202`
/// would suggest the write itself might not have happened.
pub(crate) async fn put_identity(
    State(state): State<ControlPlaneState>,
    ClientPath(client_id): ClientPath,
    operator: Operator,
    headers: HeaderMap,
    BoundedJson(request): BoundedJson<IdentityRequest>,
) -> Result<Response, ControlPlaneError> {
    let expected = preconditions::required_revision(&headers)?;
    let identity = IdentityConfiguration::from(request);

    let stored = state
        .service
        .set_identity(&operator, &client_id, identity, &expected)
        .await?;

    // After the write, and after the response is decided: convergence is a
    // separate event that fails independently, and this one borrows an
    // authority that belongs to the person who just made the request.
    crate::converge::in_background(&state, &operator);

    let reconciliation = state.service.reconciliation(&stored);
    let body = IdentityResponse::new(&stored, reconciliation);

    let mut response = (StatusCode::OK, Json(body)).into_response();

    if let Ok(tag) = HeaderValue::from_str(&preconditions::entity_tag(&stored.revision)) {
        response.headers_mut().insert(ETAG, tag);
    }

    Ok(response)
}
