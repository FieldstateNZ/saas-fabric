//! A registered repository's version tags, for the picker.

use axum::extract::State;
use axum::Json;
use serde::Serialize;

use super::path::{RegistryHostPath, RepositoryTail};
use crate::state::ControlPlaneState;
use crate::{ControlPlaneError, Operator};

/// The versions a picker offers.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Versions {
    /// Tags that are component versions, newest first.
    tags: Vec<String>,

    /// How many tags are not versions, so a picker can say some were left
    /// out rather than seem to have missed them.
    other: usize,
}

/// `GET /api/integrations/registries/{host}/versions/{*path}`
///
/// # Errors
///
/// `registry_not_found` if the repository is not registered under this
/// registry, `repository_not_readable`, or the registry's failure.
pub(crate) async fn registry_versions(
    _operator: Operator,
    State(state): State<ControlPlaneState>,
    RegistryHostPath(host): RegistryHostPath,
    RepositoryTail(repository): RepositoryTail,
) -> Result<Json<Versions>, ControlPlaneError> {
    let found = state.registries.versions(&host, &repository).await?;
    Ok(Json(Versions {
        tags: found.tags,
        other: found.other,
    }))
}
