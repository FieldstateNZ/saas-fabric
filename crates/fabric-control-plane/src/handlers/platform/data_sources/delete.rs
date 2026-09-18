//! `DELETE /api/platform/data-sources/{dataSourceId}`

use axum::extract::State;
use axum::response::{IntoResponse, Response};
use axum::Json;
use fabric_platform_management::{Declared, DesiredStateError, PlatformError};
use http::header::ETAG;
use http::{HeaderMap, HeaderValue};

use crate::extraction::DataSourceIdPath;
use crate::handlers::platform::data_sources::body::{self, DataSourcesBody};
use crate::state::ControlPlaneState;
use crate::{preconditions, ControlPlaneError, Operator};

/// Removes a declared data source, refusing while a tenant is still placed
/// on it (ADR 0023 part 2).
///
/// `If-Match` names the data-sources revision being edited, required for
/// the same reason [`declare_data_source`](super::declare_data_source) requires
/// it: the file is rewritten whole on every change, so a stale precondition
/// is refused rather than silently applied over a lost concurrent edit.
///
/// The file is never deleted -- removing the last declaration leaves an
/// empty list, and the response is the list that follows, the same as a
/// successful declaration answers with the read that followed its write.
///
/// # Errors
///
/// [`ControlPlaneError`] if this deployment manages no platform, no
/// precondition header was sent ([`ControlPlaneError::RevisionRequired`]),
/// the precondition does not match what is held
/// ([`ControlPlaneError::RevisionConflict`]), a placement still names this
/// data source (`409` `data_source_in_use`, naming every tenant, via the
/// structural arm in `errors::status_mapping::platform`), or either held
/// document -- the data sources or the placements -- is a hand edit made
/// incoherent (`500` `desired_state_invalid`; an adapter-level failure such
/// as a revoked credential gets the generic platform mapping's `503`
/// instead, exactly like hold and rollback).
pub(crate) async fn remove_data_source(
    State(state): State<ControlPlaneState>,
    _operator: Operator,
    DataSourceIdPath(id): DataSourceIdPath,
    headers: HeaderMap,
) -> Result<Response, ControlPlaneError> {
    let platform = state.platform()?;
    let expected = preconditions::required_platform_revision(&headers)?;

    let declared = platform
        .data_sources
        .remove(
            &platform.environment,
            &id,
            Some(&expected),
            platform.repository.as_ref(),
        )
        .await
        .map_err(|error| match error {
            // Same substitution `declare_data_source` makes, and for the
            // same reason: this route's precondition is the data-sources
            // ETag, so a stale one gets that write's code rather than the
            // generic platform mapping's `platform_state_moved`.
            PlatformError::DesiredState(DesiredStateError::Conflict) => ControlPlaneError::RevisionConflict,
            // `PlatformError::DataSourceInUse` and every held-document
            // failure keep their own status and code through the
            // structural arms in `errors::status_mapping::platform` and
            // `::codes` -- there is nothing left for this handler to
            // translate.
            other => ControlPlaneError::Platform(other),
        })?;

    let read = match declared {
        Declared::Written(read) | Declared::Unchanged(read) => read,
    };
    let revision = body::current_revision(&read)?;

    let mut response = Json(DataSourcesBody::of(&platform.environment, revision, &read)).into_response();

    if let Ok(tag) = HeaderValue::from_str(&preconditions::platform_entity_tag(revision)) {
        response.headers_mut().insert(ETAG, tag);
    }

    Ok(response)
}
