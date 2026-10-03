//! Registering a registry.

use axum::extract::State;
use axum::Json;
use http::StatusCode;
use serde::Deserialize;

use super::admitted::admitted;
use super::view::RegistryView;
use crate::audit::RegistryOperation;
use crate::extraction::BoundedJson;
use crate::registries::Registration;
use crate::state::ControlPlaneState;
use crate::{ControlPlaneError, Operator, RegistryKind, SecretValue};

/// What an operator registers.
///
/// # No host, and an endpoint only for `distribution`
///
/// `ghcr` and `dockerHub` are fixed by their kind, so a body naming a host
/// is refused as a field this API does not have, and one giving them an
/// endpoint is refused by the rules. The token is write-only: it goes to the
/// secret partition and no response carries it. No `Debug`, so this cannot
/// be printed whole by accident.
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(crate) struct RegisterRequest {
    /// Which kind of registry.
    kind: RegistryKind,

    /// A `distribution` registry's HTTPS origin.
    #[serde(default)]
    endpoint: Option<String>,

    /// The registry account, given with `token` or not at all.
    #[serde(default)]
    username: Option<String>,

    /// Its long-lived token.
    #[serde(default)]
    token: Option<String>,
}

/// `POST /api/integrations/registries` — `201` once it is proven and
/// recorded.
///
/// # Errors
///
/// `registry_invalid`, `registry_exists`, `registry_endpoint_differs`, or a
/// proof's failure; see `errors::status_mapping::registry`.
pub(crate) async fn register_registry(
    operator: Operator,
    State(state): State<ControlPlaneState>,
    request: Result<BoundedJson<RegisterRequest>, ControlPlaneError>,
) -> Result<(StatusCode, Json<RegistryView>), ControlPlaneError> {
    let BoundedJson(request) = admitted(&operator, RegistryOperation::Register, None, request)?;
    let registration = Registration {
        kind: request.kind,
        endpoint: request.endpoint,
        username: request.username,
        token: request.token.map(SecretValue::new),
    };
    let listed = state.registries.register(&operator, registration).await?;

    Ok((StatusCode::CREATED, Json(RegistryView::of(listed))))
}
