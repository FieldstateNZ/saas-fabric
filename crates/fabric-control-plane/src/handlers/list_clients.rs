//! `GET /api/clients`

use axum::extract::State;
use axum::Json;

use crate::models::{ClientListResponse, ClientResponse};
use crate::state::ControlPlaneState;
use crate::{ControlPlaneError, Operator};

/// Lists every client the platform manages.
///
/// The `_operator` parameter is never read, and it is not vestigial: extracting
/// it is what authenticates the request. Removing it would make this endpoint
/// public.
pub(crate) async fn list_clients(
    State(state): State<ControlPlaneState>,
    _operator: Operator,
) -> Result<Json<ClientListResponse>, ControlPlaneError> {
    let clients = state.service.list().await?;

    let lifecycles = state.lifecycles(&clients).await;

    Ok(Json(ClientListResponse {
        clients: clients
            .iter()
            .zip(lifecycles)
            .map(|(stored, lifecycle)| ClientResponse::from_stored(stored, lifecycle))
            .collect(),
    }))
}
