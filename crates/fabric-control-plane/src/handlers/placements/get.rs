//! `GET /api/clients/{clientId}/placements`

use axum::extract::State;
use axum::response::{IntoResponse, Response};
use axum::Json;
use http::header::ETAG;
use http::HeaderValue;

use crate::extraction::ClientPath;
use crate::handlers::placements::body::{self, PlacementsBody};
use crate::handlers::placements::intent::to_platform_intents;
use crate::state::ControlPlaneState;
use crate::{preconditions, ControlPlaneError, Operator};

/// Every logical data source a client's document names, and what is true
/// of each -- already placed, placeable, or refused and why (ADR 0023
/// part 2).
///
/// Reading this changes nothing, the same property `list_data_sources`
/// keeps and for the same reason: a refresh, a second operator's tab, or a
/// browser prefetch must not be able to move desired state.
///
/// # Errors
///
/// [`ControlPlaneError`] if this deployment manages no platform, there is
/// no such client, the client's own id is not a valid tenant id (an
/// invariant this platform's document creation keeps, checked anyway --
/// `fabric_platform_management::Placements` does the reparsing itself, so
/// this handler never sees a raw `TenantId`, ADR 0023 part 2 N11), or
/// either held document -- the environment's declared data sources or its
/// recorded placements -- could not be read. A held document a hand edit
/// made incoherent answers the same `500` `desired_state_invalid` a broken
/// client document already gets.
pub(crate) async fn list_placements(
    State(state): State<ControlPlaneState>,
    _operator: Operator,
    ClientPath(client_id): ClientPath,
) -> Result<Response, ControlPlaneError> {
    let platform = state.platform()?;
    let stored = state.service.get(&client_id).await?;

    let intents = &stored.document.client().data;
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
