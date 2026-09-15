//! `GET /api/activity`

use axum::extract::State;
use axum::Json;

use crate::models::ActivityResponse;
use crate::state::ControlPlaneState;
use crate::{ControlPlaneError, Operator};

/// Every recorded operator action, from the catalogue and from every client,
/// newest first.
///
/// Merged here rather than read from one document, because an operator asking
/// "what changed recently" is asking across the whole platform, not one
/// client at a time — and the two write paths already record into the same
/// [`ProductActivity`](fabric_client_model::catalogue::ProductActivity)
/// shape for exactly this reason.
pub(crate) async fn list_activity(
    _operator: Operator,
    State(state): State<ControlPlaneState>,
) -> Result<Json<ActivityResponse>, ControlPlaneError> {
    let mut activity = state.service.catalogue().await?.catalogue.activity;

    for stored in state.service.list().await? {
        activity.extend(
            stored
                .document
                .product()
                .map_err(ControlPlaneError::InvalidRequest)?
                .activity,
        );
    }

    activity.sort_by_key(|action| std::cmp::Reverse(action.at));

    Ok(Json(ActivityResponse { activity }))
}
