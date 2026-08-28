//! `GET /api/clients/{clientId}`

use axum::extract::State;
use axum::Json;

use crate::extraction::ClientPath;
use crate::models::ClientResponse;
use crate::state::ControlPlaneState;
use crate::{ControlPlaneError, Operator};

/// One client's overview.
pub(crate) async fn get_client(
    State(state): State<ControlPlaneState>,
    ClientPath(client_id): ClientPath,
    _operator: Operator,
) -> Result<Json<ClientResponse>, ControlPlaneError> {
    let stored = state.service.get(&client_id).await?;

    Ok(Json(ClientResponse::from_stored(&stored)))
}
