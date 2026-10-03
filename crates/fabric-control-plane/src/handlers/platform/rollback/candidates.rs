//! What an operator could put an environment back on.

use axum::extract::{Path, State};
use axum::Json;
use fabric_platform_management::Release;
use serde::Serialize;

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

    /// The commit every one of its images was built from — for a described
    /// component, the one commit its images and its component descriptor
    /// agree on, which is what made it a release unit (ADR 0026 section 3).
    ///
    /// **Absent for a chart, rather than empty.** A chart repository's index
    /// lists versions and no provenance, so there is no commit to name —
    /// and `""` or `null` would invite the console to render "built from"
    /// about something nothing observed.
    #[serde(skip_serializing_if = "Option::is_none")]
    source_revision: Option<String>,
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
        versions: found.releases.iter().map(CandidateRow::of).collect(),
        more: found.more,
    }))
}

impl CandidateRow {
    /// Renders one candidate, saying only what its kind can support.
    ///
    /// A described release names its commit as an image release does: both
    /// are one [`ReleaseUnit`](fabric_platform_management::ReleaseUnit), and
    /// what the component descriptor adds — its digest — is the commit
    /// message's to name, not the operator's to choose.
    fn of(release: &Release) -> Self {
        match release {
            Release::Unit(unit) | Release::Described { unit, .. } => Self {
                version: unit.version.as_str().to_owned(),
                source_revision: Some(unit.source_revision.clone()),
            },
            Release::Chart { version, .. } => Self {
                version: version.as_str().to_owned(),
                source_revision: None,
            },
        }
    }
}
