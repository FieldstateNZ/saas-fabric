//! `POST /api/clients/{clientId}/placements/{logical}`

use axum::extract::State;
use axum::response::{IntoResponse, Response};
use axum::Json;
use fabric_platform_management::{DesiredStateError, PlatformError};
use http::header::ETAG;
use http::{HeaderMap, HeaderValue};

use crate::extraction::{ClientPath, LogicalDataSourcePath};
use crate::handlers::placements::body::{self, PlacementsBody};
use crate::handlers::placements::intent::{to_platform_intent, to_platform_intents};
use crate::state::ControlPlaneState;
use crate::{preconditions, ControlPlaneError, Operator};

/// Places one logical data source's intent and records the outcome (ADR
/// 0023 part 2).
///
/// `If-Match` names the placements revision being edited, required for the
/// same reason `declare_data_source` requires one for the data sources
/// document: the file is rewritten whole on every change, so a stale
/// precondition is refused rather than silently applied over a lost
/// concurrent placement.
///
/// The intent placed is not in the request body -- it is `spec.data.<logical>`,
/// already declared in the client's document, which is what makes this an
/// act ("place what the client already asks for") rather than a second way
/// to state the same intent that could disagree with the first.
///
/// # Errors
///
/// [`ControlPlaneError`] if this deployment manages no platform, there is
/// no such client, `{logical}` does not parse as a
/// [`fabric_core::LogicalDataSourceName`] (`400`), the client's document
/// names no `spec.data` entry for it
/// ([`ControlPlaneError::LogicalDataSourceNotDeclared`], `422`
/// `placement_refused`), no `If-Match` was sent
/// ([`ControlPlaneError::RevisionRequired`]), the precondition does not
/// match what is held ([`ControlPlaneError::RevisionConflict`]), the
/// selector refuses the intent (`422` `placement_refused`, the refusal's
/// own words, via the structural arm in `errors::status_mapping::platform`),
/// or either held document is a hand edit made incoherent (`500`
/// `desired_state_invalid`).
pub(crate) async fn place_data_source(
    State(state): State<ControlPlaneState>,
    _operator: Operator,
    ClientPath(client_id): ClientPath,
    LogicalDataSourcePath(logical): LogicalDataSourcePath,
    headers: HeaderMap,
) -> Result<Response, ControlPlaneError> {
    let platform = state.platform()?;
    let expected = preconditions::required_platform_revision(&headers)?;

    let stored = state.service.get(&client_id).await?;

    let intents = &stored.document.client().data;
    let Some(intent) = intents.get(&logical) else {
        return Err(ControlPlaneError::LogicalDataSourceNotDeclared { logical });
    };

    platform
        .placements
        .place(
            &platform.environment,
            client_id.as_str(),
            &logical,
            &to_platform_intent(intent),
            Some(&expected),
        )
        .await
        .map_err(|error| match error {
            // This route's precondition is the placements ETag, so a stale
            // one gets the write's own code rather than the generic
            // platform mapping's `platform_state_moved` -- the same
            // substitution `declare_data_source` makes for its document.
            PlatformError::DesiredState(DesiredStateError::Conflict) => ControlPlaneError::RevisionConflict,
            // `PlatformError::PlacementRefused` and every held-document
            // failure keep their own status and code through the
            // structural arms in `errors::status_mapping::platform` and
            // `::codes`.
            other => ControlPlaneError::Platform(other),
        })?;

    // Read again rather than render the write's own answer: `place` returns
    // only the placements list, and the response here is the same
    // intent-plus-outcome view `list_placements` renders, for every logical
    // data source the client names -- not only the one just placed. See
    // `handlers::platform::data_sources::put`'s own doc comment for why a
    // write reads again rather than trusting what it just sent.
    let outcomes = platform
        .placements
        .for_client(
            &platform.environment,
            client_id.as_str(),
            &to_platform_intents(intents),
        )
        .await?;
    let revision = body::current_revision(&outcomes)?;

    let response_body = PlacementsBody::of(
        client_id.as_str(),
        &platform.environment,
        revision,
        intents,
        &outcomes.entries,
    );

    let mut response = Json(response_body).into_response();

    if let Ok(tag) = HeaderValue::from_str(&preconditions::platform_entity_tag(revision)) {
        response.headers_mut().insert(ETAG, tag);
    }

    Ok(response)
}
