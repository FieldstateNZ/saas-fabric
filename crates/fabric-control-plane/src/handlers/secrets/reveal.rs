//! Revealing a secret, which is an act rather than a page load.

use std::collections::BTreeMap;

use axum::extract::State;
use axum::http::header::{HeaderValue, CACHE_CONTROL};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Serialize;

use crate::extraction::{BoundedJson, ClientPath};
use crate::state::ControlPlaneState;
use crate::{ControlPlaneError, Operator};

use crate::SecretPath;
use fabric_client_model::DesiredStateError;

/// Which secret to reveal.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RevealRequest {
    /// The path, within the client.
    path: String,
}

/// The one response in this API that carries secret values.
#[derive(Serialize)]
struct Revealed {
    /// The values, by key.
    values: BTreeMap<String, String>,
}

/// `POST /api/clients/{clientId}/secrets/{path}/reveal`
///
/// # Why the response says `no-store`
///
/// Set explicitly rather than left to a default. Without it a proxy, a browser
/// cache or a disk cache is free to keep the one response in this API that
/// carries a secret — and every one of those is a copy nobody knows about and
/// nobody can revoke.
pub(crate) async fn reveal_secret(
    State(state): State<ControlPlaneState>,
    ClientPath(client): ClientPath,
    operator: Operator,
    BoundedJson(request): BoundedJson<RevealRequest>,
) -> Result<Response, ControlPlaneError> {
    let path = SecretPath::parse(&request.path).map_err(|detail| {
        ControlPlaneError::InvalidRequest(DesiredStateError::InvalidField {
            field: "path",
            detail,
        })
    })?;

    let values = state.secrets()?.reveal(&operator, &client, &path).await?;

    let mut response = Json(Revealed {
        values: values.revealed().clone(),
    })
    .into_response();

    response.headers_mut().insert(
        CACHE_CONTROL,
        HeaderValue::from_static("no-store, no-cache, must-revalidate"),
    );

    Ok(response)
}
