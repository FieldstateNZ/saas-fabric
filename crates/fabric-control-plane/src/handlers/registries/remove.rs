//! Removing a registry.

use axum::extract::State;
use http::StatusCode;

use super::admitted::admitted;
use super::path::RegistryHostPath;
use crate::audit::RegistryOperation;
use crate::state::ControlPlaneState;
use crate::{ControlPlaneError, Operator};

/// `DELETE /api/integrations/registries/{host}` — `204`.
///
/// # Errors
///
/// `registry_not_found`, or a store failure.
pub(crate) async fn remove_registry(
    operator: Operator,
    State(state): State<ControlPlaneState>,
    host: Result<RegistryHostPath, ControlPlaneError>,
) -> Result<StatusCode, ControlPlaneError> {
    let RegistryHostPath(host) = admitted(&operator, RegistryOperation::Remove, None, host)?;
    state.registries.remove(&operator, host).await?;
    Ok(StatusCode::NO_CONTENT)
}
