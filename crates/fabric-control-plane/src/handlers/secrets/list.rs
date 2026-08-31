//! Listing a client's secrets, and looking at one without revealing it.

use axum::extract::State;
use axum::Json;
use serde::Serialize;

use crate::extraction::ClientPath;
use crate::state::ControlPlaneState;
use crate::{ControlPlaneError, Operator, SecretMetadata};

use super::super::secrets_path::SecretPathTail;

/// One entry in a client's secret listing.
///
/// A path and nothing else. Versions and timestamps come from the metadata
/// call, because a listing that fetched them would make one request per secret
/// on a page that exists to show many.
#[derive(Serialize)]
pub(crate) struct SecretEntry {
    /// Where the secret is, within the client.
    path: String,
}

/// `GET /api/clients/{clientId}/secrets`
pub(crate) async fn list_secrets(
    State(state): State<ControlPlaneState>,
    ClientPath(client): ClientPath,
    _operator: Operator,
) -> Result<Json<Vec<SecretEntry>>, ControlPlaneError> {
    let paths = state.secrets()?.list(&client).await?;

    Ok(Json(
        paths
            .into_iter()
            .map(|path| SecretEntry {
                path: path.to_string(),
            })
            .collect(),
    ))
}

/// `GET /api/clients/{clientId}/secrets/{path}/metadata`
///
/// Carries no values, which is what lets a console call it freely.
pub(crate) async fn secret_metadata(
    State(state): State<ControlPlaneState>,
    ClientPath(client): ClientPath,
    SecretPathTail(path): SecretPathTail,
    _operator: Operator,
) -> Result<Json<SecretMetadata>, ControlPlaneError> {
    Ok(Json(state.secrets()?.metadata(&client, &path).await?))
}
