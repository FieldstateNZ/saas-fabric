//! Listing the registries, and the deployment's own.

use axum::extract::State;
use axum::Json;
use serde::Serialize;

use super::view::RegistryView;
use crate::state::ControlPlaneState;
use crate::{ControlPlaneError, DeploymentRegistry, Operator};

/// Every registry, and the deployment's own.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Listing {
    /// The registries operators registered.
    registries: Vec<RegistryView>,

    /// The deployment's own registry, when Platform Management is configured:
    /// listed as the deployment's, because it is configuration.
    deployment: Option<DeploymentView>,
}

/// The deployment's own registry.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DeploymentView {
    /// How its repositories are named.
    host: String,

    /// Where it is served.
    endpoint: String,
}

impl DeploymentView {
    /// The view of `deployment`.
    fn of(deployment: &DeploymentRegistry) -> Self {
        Self {
            host: deployment.host().to_owned(),
            endpoint: deployment.endpoint().to_owned(),
        }
    }
}

/// `GET /api/integrations/registries`
///
/// # Errors
///
/// `registries_unavailable` or `registries_invalid` if the record set could
/// not be read. A failure, not an empty list: "none registered" and "could
/// not look" send an operator to different places.
pub(crate) async fn list_registries(
    _operator: Operator,
    State(state): State<ControlPlaneState>,
) -> Result<Json<Listing>, ControlPlaneError> {
    let (listed, deployment) = state.registries.list().await?;

    Ok(Json(Listing {
        registries: listed.into_iter().map(RegistryView::of).collect(),
        deployment: deployment.map(DeploymentView::of),
    }))
}
