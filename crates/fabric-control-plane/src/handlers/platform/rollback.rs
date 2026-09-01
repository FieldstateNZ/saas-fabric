//! Putting an environment back on something it ran before.

use axum::extract::{Path, State};
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::handlers::platform::body::ComponentRow;
use crate::state::ControlPlaneState;
use crate::{ControlPlaneError, Operator};

/// One version an operator could go back to.
///
/// The version and what it was built from, and nothing else. Not the digests:
/// an operator does not choose those and the API must not invite anything to
/// send them back — what gets written is resolved by the platform at the
/// moment of the write.
#[derive(Serialize)]
pub(crate) struct CandidateRow {
    /// The version, as it is tagged.
    version: String,

    /// The commit every one of its images was built from.
    source_revision: String,
}

/// What an operator is offered.
#[derive(Serialize)]
pub(crate) struct CandidatesBody {
    /// Complete, coherent versions below the desired one, newest first.
    versions: Vec<CandidateRow>,

    /// Whether older versions exist that were not examined.
    ///
    /// Reported rather than hidden. A list that quietly stopped would read as
    /// "this is everything there is".
    more: bool,
}

/// Which version, and why.
#[derive(Deserialize)]
pub(crate) struct Rollback {
    /// One of the versions the candidates listing offered.
    ///
    /// Checked against what was observed, never trusted: a version the
    /// platform does not already hold a resolved release unit for is refused.
    version: String,

    /// Free text, shown beside the hold and never branched on.
    #[serde(default)]
    note: Option<String>,
}

/// `GET /api/platform/components/{component}/versions`.
///
/// # Errors
///
/// [`ControlPlaneError`] if this deployment manages no platform, the manifest
/// does not name this component, or a registry could not be asked.
pub(crate) async fn rollback_candidates(
    State(state): State<ControlPlaneState>,
    _operator: Operator,
    Path(component): Path<String>,
) -> Result<Json<CandidatesBody>, ControlPlaneError> {
    let platform = state.platform()?;

    let found = platform
        .service
        .rollback_candidates(&platform.environment, &component)
        .await?;

    Ok(Json(CandidatesBody {
        versions: found
            .units
            .iter()
            .map(|unit| CandidateRow {
                version: unit.version.as_str().to_owned(),
                source_revision: unit.source_revision.clone(),
            })
            .collect(),
        more: found.more,
    }))
}

/// `POST /api/platform/components/{component}/rollback`.
///
/// # It is a POST because it is an act
///
/// Not a `PUT` of desired state. An operator is not replacing a resource with
/// one they composed; they are asking the platform to do something, and what
/// gets written — three digests and a hold — is the platform's to determine.
///
/// # Errors
///
/// [`ControlPlaneError`] if this deployment manages no platform, the manifest
/// does not name this component, the version is not one it can be rolled back
/// to, or the write could not be made.
pub(crate) async fn roll_back_component(
    State(state): State<ControlPlaneState>,
    _operator: Operator,
    Path(component): Path<String>,
    Json(body): Json<Rollback>,
) -> Result<Json<ComponentRow>, ControlPlaneError> {
    let platform = state.platform()?;

    let status = platform
        .service
        .roll_back(
            &platform.environment,
            &component,
            &body.version,
            body.note.as_deref(),
        )
        .await?;

    Ok(Json(ComponentRow::of(&status)))
}
