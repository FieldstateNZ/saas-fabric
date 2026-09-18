//! What an operator is told about an environment's composition.

mod body;
mod brake;
// `pub(crate)` rather than private: `handlers::placements` (ADR 0023 part
// 2) reuses `data_sources::placement::console_word`, so this module and
// `placement` within it both have to be reachable from outside `platform`'s
// own subtree.
pub(crate) mod data_sources;
mod rollback;

pub(crate) use brake::{pause_component, resume_component};
pub(crate) use data_sources::{declare_data_source, list_data_sources, remove_data_source};
pub(crate) use rollback::{roll_back_component, rollback_candidates};

use axum::extract::State;
use axum::Json;

use crate::handlers::platform::body::PlatformBody;
use crate::state::ControlPlaneState;
use crate::{ControlPlaneError, Operator};

/// `GET /api/platform`.
///
/// # It takes no environment
///
/// A deployment manages the environment it was deployed into, stated in its
/// configuration. An environment name reaches the platform repository as a
/// path segment, so a caller who could name one could name a path — and the
/// cheapest way to satisfy section 31.7 is to give them nowhere to say it.
///
/// # Reading this cannot change anything
///
/// It calls `statuses`, which has no path to a write. Refreshing the page, a
/// second operator opening it, or a browser prefetching it must not move an
/// environment — and the property that makes that true is structural rather
/// than a promise made here.
///
/// What advances an environment is the background sweep, on the cadence its
/// deployment configures.
///
/// # Errors
///
/// [`ControlPlaneError`] if Platform Management is not configured for this
/// deployment, or the environment cannot be read.
pub(crate) async fn get_platform(
    State(state): State<ControlPlaneState>,
    _operator: Operator,
) -> Result<Json<PlatformBody>, ControlPlaneError> {
    let platform = state.platform()?;

    let components = platform.service.statuses(&platform.environment).await?;

    Ok(Json(PlatformBody::of(
        &platform.environment,
        components.as_slice(),
        state.platform_sweeps.last_check().as_ref(),
    )))
}
