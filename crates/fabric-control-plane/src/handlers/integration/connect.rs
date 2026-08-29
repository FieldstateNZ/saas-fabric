//! Starting each leg of the connection, as an authenticated operator.

use axum::extract::State;
use axum::Json;
use serde::Deserialize;

use crate::state::ControlPlaneState;
use crate::{ControlPlaneError, Operator};

/// Which organisation the application should be created in.
#[derive(Deserialize)]
pub(crate) struct Organisation {
    /// The account or organisation name on the Git host.
    organisation: String,
}

/// Describes the application to create, and where the browser must post it.
///
/// The response is not a redirect. Creating an application through a manifest
/// requires an HTTP **POST** of a form field, which a browser can only do by
/// submitting a real form — so the console is handed what to post and does it
/// itself.
pub(crate) async fn begin_connection(
    operator: Operator,
    State(state): State<ControlPlaneState>,
    Json(body): Json<Organisation>,
) -> Result<Json<serde_json::Value>, ControlPlaneError> {
    let request = state
        .git_integration()?
        .begin_connection(&operator, &body.organisation)?;

    Ok(Json(serde_json::json!(request)))
}

/// Where the operator installs the application once it exists.
pub(crate) async fn begin_install(
    operator: Operator,
    State(state): State<ControlPlaneState>,
) -> Result<Json<serde_json::Value>, ControlPlaneError> {
    let url = state.git_integration()?.begin_install(&operator).await?;

    Ok(Json(serde_json::json!({ "url": url })))
}

/// Forgets the integration this platform holds.
pub(crate) async fn disconnect(
    operator: Operator,
    State(state): State<ControlPlaneState>,
) -> Result<http::StatusCode, ControlPlaneError> {
    state.git_integration()?.disconnect(&operator).await?;

    Ok(http::StatusCode::NO_CONTENT)
}
