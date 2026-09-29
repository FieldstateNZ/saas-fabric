//! Setting, replacing and removing a registry's credential.

use axum::extract::State;
use axum::Json;
use serde::Deserialize;

use super::admitted::admitted;
use super::path::RegistryHostPath;
use super::view::RegistryView;
use crate::audit::RegistryOperation;
use crate::extraction::BoundedJson;
use crate::state::ControlPlaneState;
use crate::{ControlPlaneError, Operator, SecretValue};

/// A credential. The token is write-only, and no `Debug` exists to print it.
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(crate) struct CredentialRequest {
    /// The registry account.
    username: String,

    /// Its long-lived token.
    token: String,
}

/// `PUT /api/integrations/registries/{host}/credential` — once the registry,
/// and every repository registered under it, proves with it.
///
/// # Errors
///
/// `registry_not_found`, `registry_invalid`, or a proof's failure.
pub(crate) async fn set_registry_credential(
    operator: Operator,
    State(state): State<ControlPlaneState>,
    host: Result<RegistryHostPath, ControlPlaneError>,
    request: Result<BoundedJson<CredentialRequest>, ControlPlaneError>,
) -> Result<Json<RegistryView>, ControlPlaneError> {
    let operation = RegistryOperation::SetCredential;
    let RegistryHostPath(host) = admitted(&operator, operation, None, host)?;
    let BoundedJson(request) = admitted(&operator, operation, Some(&host), request)?;
    let listed = state
        .registries
        .set_credential(&operator, host, request.username, SecretValue::new(request.token))
        .await?;
    Ok(Json(RegistryView::of(listed)))
}

/// `DELETE /api/integrations/registries/{host}/credential`
///
/// # Errors
///
/// `registry_not_found`, or a store failure.
pub(crate) async fn remove_registry_credential(
    operator: Operator,
    State(state): State<ControlPlaneState>,
    host: Result<RegistryHostPath, ControlPlaneError>,
) -> Result<Json<RegistryView>, ControlPlaneError> {
    let RegistryHostPath(host) = admitted(&operator, RegistryOperation::RemoveCredential, None, host)?;
    let listed = state.registries.remove_credential(&operator, host).await?;
    Ok(Json(RegistryView::of(listed)))
}
