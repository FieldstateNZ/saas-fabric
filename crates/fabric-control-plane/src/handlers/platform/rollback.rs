//! Putting an environment back on an older published version.

mod candidates;

pub(crate) use candidates::rollback_candidates;

use axum::extract::{Path, State};
use axum::Json;
use serde::Deserialize;

use crate::handlers::platform::body::ComponentRow;
use crate::state::ControlPlaneState;
use crate::{ControlPlaneError, Operator};

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
