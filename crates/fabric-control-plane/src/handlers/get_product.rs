//! `GET /api/clients/{clientId}/product`

use axum::extract::State;
use axum::response::{IntoResponse, Response};
use axum::Json;
use http::header::ETAG;
use http::HeaderValue;

use crate::extraction::ClientPath;
use crate::models::ClientProductResponse;
use crate::state::ControlPlaneState;
use crate::{preconditions, ControlPlaneError, Operator};

/// A client's product configuration and what it resolves to.
pub(crate) async fn get_product(
    _operator: Operator,
    State(state): State<ControlPlaneState>,
    ClientPath(id): ClientPath,
) -> Result<Response, ControlPlaneError> {
    let stored = state.service.get(&id).await?;
    let reconciliation = state.service.reconciliation(&stored);
    let body = ClientProductResponse::from_stored(&stored, reconciliation)?;

    let mut response = Json(body).into_response();

    if let Ok(tag) = HeaderValue::from_str(&preconditions::entity_tag(&stored.revision)) {
        response.headers_mut().insert(ETAG, tag);
    }

    Ok(response)
}
