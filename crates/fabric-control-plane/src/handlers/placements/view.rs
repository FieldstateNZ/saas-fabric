//! The response both placements routes answer with: every logical data
//! source the client's document names, what is true of each, and which
//! catalogue resources the client's tenant cannot reach (G8).

use std::collections::BTreeMap;

use axum::response::{IntoResponse, Response};
use axum::Json;
use fabric_core::LogicalDataSourceName;
use http::header::ETAG;
use http::HeaderValue;

use crate::handlers::placements::body::{self, PlacementsBody, UnboundBody};
use crate::handlers::placements::intent::to_platform_intents;
use crate::state::ControlPlaneState;
use crate::{preconditions, ControlPlaneError, PlatformBinding};
use fabric_client_model::ClientId;

/// Reads the client's placements and the derived runtime catalogue, and
/// renders both with the placements revision as `ETag`.
///
/// # Errors
///
/// [`ControlPlaneError`] if either held platform document cannot be read
/// or is incoherent. A catalogue that cannot be read or derived does not
/// fail the response: placing data does not depend on it, so the body
/// says the check could not be made, and why.
pub(super) async fn placements_view(
    state: &ControlPlaneState,
    platform: &PlatformBinding,
    client_id: &ClientId,
    intents: &BTreeMap<LogicalDataSourceName, fabric_client_model::DataIntent>,
) -> Result<Response, ControlPlaneError> {
    let outcomes = platform
        .placements
        .for_client(
            &platform.environment,
            client_id.as_str(),
            &to_platform_intents(intents),
        )
        .await?;
    let revision = body::current_revision(&outcomes)?;

    let unbound = match state.service.catalogue().await {
        Ok(stored) => match stored.catalogue.runtime_catalogue() {
            Ok(derived) => UnboundBody::checked(&derived.unbound_for(&outcomes.bound), intents),
            Err(conflict) => UnboundBody::Unchecked {
                reason: conflict.to_string(),
            },
        },
        Err(error) => UnboundBody::Unchecked {
            reason: error.public_message(),
        },
    };

    let response_body = PlacementsBody::of(
        client_id.as_str(),
        &platform.environment,
        revision,
        intents,
        &outcomes.entries,
        unbound,
    );

    let mut response = Json(response_body).into_response();
    if let Ok(tag) = HeaderValue::from_str(&preconditions::platform_entity_tag(revision)) {
        response.headers_mut().insert(ETAG, tag);
    }
    Ok(response)
}
