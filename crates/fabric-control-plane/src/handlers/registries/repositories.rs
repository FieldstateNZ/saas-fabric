//! Registering and removing a registry's repositories.

use axum::extract::State;
use axum::Json;

use super::admitted::admitted;
use super::path::{RegistryHostPath, RepositoryTail};
use super::view::RegistryView;
use crate::audit::RegistryOperation;
use crate::state::ControlPlaneState;
use crate::{ControlPlaneError, Operator};

/// `PUT /api/integrations/registries/{host}/repositories/entry/{*path}` —
/// once its tag listing answers through this registry.
///
/// # Errors
///
/// `registry_not_found`, `registry_invalid`, `repository_not_readable`, or
/// the registry's failure.
pub(crate) async fn add_registry_repository(
    operator: Operator,
    State(state): State<ControlPlaneState>,
    host: Result<RegistryHostPath, ControlPlaneError>,
    repository: Result<RepositoryTail, ControlPlaneError>,
) -> Result<Json<RegistryView>, ControlPlaneError> {
    let operation = RegistryOperation::AddRepository;
    let RegistryHostPath(host) = admitted(&operator, operation, None, host)?;
    let RepositoryTail(repository) = admitted(&operator, operation, Some(&host), repository)?;
    let listed = state
        .registries
        .add_repository(&operator, host, repository)
        .await?;
    Ok(Json(RegistryView::of(listed)))
}

/// `DELETE /api/integrations/registries/{host}/repositories/entry/{*path}`
///
/// # Errors
///
/// `registry_not_found`, or a store failure.
pub(crate) async fn remove_registry_repository(
    operator: Operator,
    State(state): State<ControlPlaneState>,
    host: Result<RegistryHostPath, ControlPlaneError>,
    repository: Result<RepositoryTail, ControlPlaneError>,
) -> Result<Json<RegistryView>, ControlPlaneError> {
    let operation = RegistryOperation::RemoveRepository;
    let RegistryHostPath(host) = admitted(&operator, operation, None, host)?;
    let RepositoryTail(repository) = admitted(&operator, operation, Some(&host), repository)?;
    let listed = state
        .registries
        .remove_repository(&operator, host, repository)
        .await?;
    Ok(Json(RegistryView::of(listed)))
}
