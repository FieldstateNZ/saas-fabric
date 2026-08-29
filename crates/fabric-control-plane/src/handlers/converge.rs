//! `POST /api/reconciliation`

use axum::extract::State;
use axum::Json;

use crate::state::ControlPlaneState;
use crate::{ControlPlaneError, Operator};

/// Converges every client onto desired state, as this operator.
///
/// # Why this endpoint exists at all
///
/// It replaces a background loop. That loop swept on an interval using a
/// service account's credential, which is exactly the standing authority ADR
/// 0012 removed — so sweeping is now something an operator does, with their
/// own permission, and this is the door.
///
/// It is also how drift is found. The platform cannot notice a realm changed
/// outside SaaS Fabric while nobody is looking, so "look now" had to become an
/// action rather than a schedule.
pub(crate) async fn converge(
    operator: Operator,
    State(state): State<ControlPlaneState>,
) -> Result<Json<serde_json::Value>, ControlPlaneError> {
    let swept = crate::converge::sweep(&state, &operator).await?;

    Ok(Json(serde_json::json!({ "clients": swept })))
}
