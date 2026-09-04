//! Putting an environment back on an older published version.

use axum::extract::{Path, State};
use axum::Json;
use fabric_platform_management::Release;
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

/// Which version, and why.
///
/// # `deny_unknown_fields`, deliberately
///
/// A body carrying a digest, a source revision, or anything else the browser
/// saw in the candidates listing is **refused**, not ignored. The temptation
/// is real and will look like a performance fix: the console has just fetched
/// those values, so why make the platform resolve them again?
///
/// Because a value the browser sends is a value an attacker can send. The
/// version is a *name*, checked against the registry at the moment of the
/// write; a digest would be the thing actually deployed, taken on trust from a
/// caller. Silently dropping an unexpected field would let that change land
/// looking like it worked.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Rollback {
    /// One of the versions the candidates listing offered.
    ///
    /// A name and nothing else. It is re-resolved against the registry on this
    /// request — not looked up in whatever the console fetched moments ago —
    /// so a version withdrawn in between is refused rather than deployed from
    /// a stale candidate object.
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
        versions: found.releases.iter().map(CandidateRow::of).collect(),
        more: found.more,
    }))
}

impl CandidateRow {
    /// Renders one candidate, saying only what its kind can support.
    fn of(release: &Release) -> Self {
        match release {
            Release::Unit(unit) => Self {
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
