//! `PUT /api/platform/data-sources/{dataSourceId}`

use axum::extract::State;
use axum::response::{IntoResponse, Response};
use axum::Json;
use fabric_platform_management::{Declared, DesiredStateError, PlatformError};
use http::header::ETAG;
use http::{HeaderMap, HeaderValue};

use crate::extraction::{BoundedJson, DataSourceIdPath};
use crate::handlers::platform::data_sources::body::{self, DataSourcesBody};
use crate::handlers::platform::data_sources::request::DataSourceInput;
use crate::state::ControlPlaneState;
use crate::{preconditions, ControlPlaneError, Operator};

/// Declares a data source, or corrects an already-declared one.
///
/// `If-Match` names the revision being corrected, and is required on every
/// call, including the first data source an environment ever declares: the
/// late-bound binding tags an empty environment the same way it tags one
/// with something to correct, so `GET /api/platform/data-sources` always
/// has a revision to read first and send back here. `If-None-Match: *` is
/// not accepted on this route -- carrying it without `If-Match` is refused
/// the same as sending no precondition at all.
///
/// A declaration that matches what is already held, once its `id` and
/// `revision` are set aside, writes nothing and still answers `200` with
/// the read that found that out (`DataSources::declare`'s `Unchanged`): an
/// operator resubmitting a form they have not changed is not a failure.
///
/// # Errors
///
/// [`ControlPlaneError`] if this deployment manages no platform, the body
/// does not parse or names a placement word this platform does not have,
/// no precondition header was sent ([`ControlPlaneError::RevisionRequired`]),
/// the precondition does not match what is held
/// ([`ControlPlaneError::RevisionConflict`] -- the same
/// ETag-preconditioned-write code a catalogue write answers, not the
/// generic platform mapping's `platform_state_moved`), the declaration
/// breaks one of ADR 0023 part 1's rules (`422` `invalid_data_source`,
/// mapped structurally in `errors::status_mapping::platform`), or the
/// document already held is a hand edit made incoherent -- a duplicate
/// id, or an entry that no longer validates (`500` `desired_state_invalid`,
/// the same answer a broken client document already gets; an
/// adapter-level failure such as a revoked credential gets the generic
/// platform mapping's `503` instead, exactly like hold and rollback).
pub(crate) async fn declare_data_source(
    State(state): State<ControlPlaneState>,
    _operator: Operator,
    DataSourceIdPath(id): DataSourceIdPath,
    headers: HeaderMap,
    BoundedJson(input): BoundedJson<DataSourceInput>,
) -> Result<Response, ControlPlaneError> {
    let platform = state.platform()?;
    let expected = preconditions::required_platform_revision(&headers)?;
    let declaration = input.into_declaration(id)?;

    let declared = platform
        .data_sources
        .declare(&platform.environment, declaration, Some(&expected))
        .await
        .map_err(|error| match error {
            // A declaration is an ETag-preconditioned write exactly like a
            // catalogue write, so a stale precondition gets that write's
            // code (409, `revision_conflict`) rather than the generic
            // platform mapping's `platform_state_moved` -- the console
            // keys its reload affordance on this one. Hold and rollback
            // are unrelated writes and keep the generic mapping; only
            // this handler makes the substitution.
            PlatformError::DesiredState(DesiredStateError::Conflict) => ControlPlaneError::RevisionConflict,
            // Every other failure, including `PlatformError::InvalidDataSource`
            // and `PlatformError::InvalidHeldDataSources`, keeps its own
            // status and code through the structural arms in
            // `errors::status_mapping::platform` and `::codes` -- there is
            // nothing left for this handler to translate. In particular an
            // adapter's own `DesiredStateError::Refused` (a revoked
            // credential, a host rejecting a write) falls to the generic
            // platform mapping's `503`, exactly like hold and rollback --
            // it is not the same failure `check_held` answers, and must
            // not be told to an operator as a broken document.
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
