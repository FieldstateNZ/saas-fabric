//! Writing a secret, against the version the operator was looking at.

use std::collections::BTreeMap;

use axum::extract::State;
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::extraction::{BoundedJson, ClientPath};
use crate::state::ControlPlaneState;
use crate::{ControlPlaneError, Operator, SecretValues};

use super::super::secrets_path::SecretPathTail;

/// What an operator submits.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WriteRequest {
    /// The values to store. A replacement, not a patch: a partial write would
    /// silently drop keys the operator could not see on the page.
    values: BTreeMap<String, String>,

    /// The version the operator was looking at, or absent for a new secret.
    ///
    /// Required in the sense that omitting it means "I believe this does not
    /// exist" — not "overwrite whatever is there". There is no way to spell
    /// the second, deliberately.
    #[serde(default)]
    expected_version: Option<u64>,
}

/// The version a write produced.
#[derive(Serialize)]
pub(crate) struct WriteResponse {
    /// The version now stored.
    version: u64,
}

/// `PUT /api/clients/{clientId}/secrets/{path}`
pub(crate) async fn write_secret(
    State(state): State<ControlPlaneState>,
    ClientPath(client): ClientPath,
    SecretPathTail(path): SecretPathTail,
    operator: Operator,
    BoundedJson(request): BoundedJson<WriteRequest>,
) -> Result<Json<WriteResponse>, ControlPlaneError> {
    let version = state
        .secrets()?
        .write(
            &operator,
            &client,
            &path,
            &SecretValues::new(request.values),
            request.expected_version,
        )
        .await?;

    Ok(Json(WriteResponse { version }))
}
