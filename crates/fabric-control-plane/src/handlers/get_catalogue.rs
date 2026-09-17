//! `GET /api/catalogue`

use axum::extract::State;
use axum::Json;
use fabric_client_model::catalogue::StoredCatalogue;

use crate::state::ControlPlaneState;
use crate::{ControlPlaneError, Operator};

/// The current product catalogue.
pub(crate) async fn get_catalogue(
    _operator: Operator,
    State(state): State<ControlPlaneState>,
) -> Result<Json<StoredCatalogue>, ControlPlaneError> {
    Ok(Json(state.service.catalogue().await?))
}
