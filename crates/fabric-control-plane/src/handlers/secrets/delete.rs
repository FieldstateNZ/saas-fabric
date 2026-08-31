//! Removing a secret, and every version of it.

use axum::extract::State;
use axum::http::StatusCode;

use crate::extraction::ClientPath;
use crate::state::ControlPlaneState;
use crate::{ControlPlaneError, Operator};

use super::super::secrets_path::SecretPathTail;

/// `DELETE /api/clients/{clientId}/secrets/{path}`
pub(crate) async fn delete_secret(
    State(state): State<ControlPlaneState>,
    ClientPath(client): ClientPath,
    SecretPathTail(path): SecretPathTail,
    operator: Operator,
) -> Result<StatusCode, ControlPlaneError> {
    state.secrets()?.delete(&operator, &client, &path).await?;

    Ok(StatusCode::NO_CONTENT)
}
