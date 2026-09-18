//! `GET /api/platform/data-sources`

use axum::extract::State;
use axum::response::{IntoResponse, Response};
use axum::Json;
use http::header::ETAG;
use http::HeaderValue;

use crate::handlers::platform::data_sources::body::{self, DataSourcesBody};
use crate::state::ControlPlaneState;
use crate::{preconditions, ControlPlaneError, Operator};

/// Every data source declared for this deployment's environment.
///
/// Reading this changes nothing — the same property `get_platform` keeps,
/// and for the same reason: a refresh, a second operator's tab, or a
/// browser prefetch must not be able to move desired state.
///
/// `revision` and `ETag` are always present, even when `dataSources` is
/// empty: the late-bound binding tags an environment with nothing declared
/// yet the same way it tags one with something to correct, so the first
/// declaration this environment ever makes has something to send back as
/// `If-Match`.
///
/// # Errors
///
/// [`ControlPlaneError`] if this deployment manages no platform, or the
/// environment's declared data sources could not be read. A held document a
/// hand edit made incoherent — a duplicate id, or an entry that no longer
/// validates — answers the same `500` `desired_state_invalid` a broken
/// client document already gets, via the structural arm in
/// `errors::status_mapping::platform`; an adapter-level failure such as a
/// revoked credential or an unreadable file gets the generic platform
/// mapping's `503`, exactly like hold and rollback.
pub(crate) async fn list_data_sources(
    State(state): State<ControlPlaneState>,
    _operator: Operator,
) -> Result<Response, ControlPlaneError> {
    let platform = state.platform()?;

    let read = platform.data_sources.list(&platform.environment).await?;
    let revision = body::current_revision(&read)?;

    let mut response = Json(DataSourcesBody::of(&platform.environment, revision, &read)).into_response();

    if let Ok(tag) = HeaderValue::from_str(&preconditions::platform_entity_tag(revision)) {
        response.headers_mut().insert(ETAG, tag);
    }

    Ok(response)
}
