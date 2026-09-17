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
///
/// # One unreadable client fails the whole listing
///
/// The same choice [`list_clients`](crate::handlers::list_clients) already
/// makes for `GET /api/clients`, and for the same reason: this is a
/// cross-client read, and there is no contract today for a response that is
/// "every event except the ones from the client whose document would not
/// parse" — a console reading a partial feed would have no way to tell a
/// short quiet day from a hidden failure. A stored document this model
/// cannot read is [`ControlPlaneError::InvalidDesiredState`], the platform's
/// problem and not the operator's, so it is reported with that error rather
/// than [`ControlPlaneError::InvalidRequest`].
pub(crate) async fn list_activity(
    _operator: Operator,
    State(state): State<ControlPlaneState>,
) -> Result<Json<ActivityResponse>, ControlPlaneError> {
    let mut activity = state.service.catalogue().await?.catalogue.activity;

    for stored in state.service.list().await? {
        let client = stored.document.client().id.clone();
        let product = stored
            .document
            .product()
            .map_err(|source| ControlPlaneError::InvalidDesiredState { client, source })?;

        activity.extend(product.activity);
    }

    activity.sort_by_key(|action| std::cmp::Reverse(action.at));

    Ok(Json(ActivityResponse { activity }))
}
