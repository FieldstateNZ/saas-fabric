//! Starting each leg of the connection, as an authenticated operator.

use axum::extract::State;
use axum::Json;
use serde::Deserialize;

use super::flow::Flow;
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
pub(crate) async fn begin_connection<F: Flow>(
    operator: Operator,
    State(state): State<ControlPlaneState>,
    Json(body): Json<Organisation>,
) -> Result<Json<serde_json::Value>, ControlPlaneError> {
    let request = F::service(&state)?.begin_connection(&operator, &body.organisation)?;

    Ok(Json(serde_json::json!(request)))
}

/// Where the operator installs the application once it exists.
pub(crate) async fn begin_install<F: Flow>(
    operator: Operator,
    State(state): State<ControlPlaneState>,
) -> Result<Json<serde_json::Value>, ControlPlaneError> {
    let url = F::service(&state)?.begin_install(&operator).await?;

    Ok(Json(serde_json::json!({ "url": url })))
}

/// Forgets the integration this platform holds.
///
/// This flow's, and no other's. The service was built for one
/// [`IntegrationKind`](crate::IntegrationKind) and reads and writes only that
/// kind's record, so forgetting one cannot reach the other's key or unbind
/// what the other connected.
pub(crate) async fn disconnect<F: Flow>(
    operator: Operator,
    State(state): State<ControlPlaneState>,
) -> Result<http::StatusCode, ControlPlaneError> {
    F::service(&state)?.disconnect(&operator).await?;

    Ok(http::StatusCode::NO_CONTENT)
}
