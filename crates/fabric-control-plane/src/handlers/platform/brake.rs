//! Stopping an environment advancing, and letting it go again.

use axum::extract::{Path, State};
use axum::Json;
use serde::Deserialize;

use crate::handlers::platform::body::ComponentRow;
use crate::state::ControlPlaneState;
use crate::{ControlPlaneError, Operator};

/// What an operator wants the next person to know.
#[derive(Deserialize)]
pub(crate) struct Pause {
    /// Free text, shown beside the pause and never branched on.
    ///
    /// The *reason* is not here. Fabric writes that from a closed vocabulary,
    /// because a later reader decides from it — and deciding on prose an
    /// operator typed is how a manifest field becomes unparseable.
    #[serde(default)]
    note: Option<String>,
}

/// `PUT /api/platform/components/{component}/hold`.
///
/// # The component is named, and that is allowed
///
/// Unlike the environment, which used to be a path segment here and reached
/// the platform repository as one, a component name is **a lookup key into a
/// manifest this platform already read and trusts**. It selects an entry;
/// it never becomes a repository path, a file path, a registry location, or
/// any other locator. A name the manifest does not carry selects nothing, and
/// that refusal is the whole of the check.
///
/// # Errors
///
/// [`ControlPlaneError`] if this deployment manages no platform, the manifest
/// does not name this component, it is not one that advances, or the write
/// could not be made.
pub(crate) async fn pause_component(
    State(state): State<ControlPlaneState>,
    _operator: Operator,
    Path(component): Path<String>,
    Json(body): Json<Pause>,
) -> Result<Json<ComponentRow>, ControlPlaneError> {
    let platform = state.platform()?;

    let status = platform
        .service
        .pause(&platform.environment, &component, body.note.as_deref())
        .await?;

    Ok(Json(ComponentRow::of(&status)))
}

/// `DELETE /api/platform/components/{component}/hold`.
///
/// Lifts the hold and nothing else. It does not advance the component: the
/// next sweep decides that, from what it observes then.
///
/// # Errors
///
/// [`ControlPlaneError`] if this deployment manages no platform, the manifest
/// does not name this component, or the write could not be made.
pub(crate) async fn resume_component(
    State(state): State<ControlPlaneState>,
    _operator: Operator,
    Path(component): Path<String>,
) -> Result<Json<ComponentRow>, ControlPlaneError> {
    let platform = state.platform()?;

    let status = platform.service.resume(&platform.environment, &component).await?;

    Ok(Json(ComponentRow::of(&status)))
}
