//! `POST /api/catalogue`

use axum::extract::State;
use axum::Json;
use fabric_client_model::catalogue::{CatalogueCommand, StoredCatalogue};
use http::HeaderMap;

use crate::extraction::BoundedJson;
use crate::state::ControlPlaneState;
use crate::{preconditions, ControlPlaneError, Operator};

/// Applies one command to the product catalogue.
///
/// One endpoint rather than a `PUT` of the whole document, because an
/// operator's action — publish this application, add this field — is what
/// they mean; a whole-document replace would ask a console to reconstruct
/// that intent from a diff, the same way `PUT /api/clients/{id}/identity`
/// reconstructs a client's intent from realms and roles rather than a raw
/// document (§8).
///
/// `If-None-Match: *` is accepted here, in addition to `If-Match`, because
/// the very first write has no prior revision to name — see
/// [`preconditions::optional_revision`].
pub(crate) async fn change_catalogue(
    operator: Operator,
    State(state): State<ControlPlaneState>,
    headers: HeaderMap,
    BoundedJson(command): BoundedJson<CatalogueCommand>,
) -> Result<Json<StoredCatalogue>, ControlPlaneError> {
    let expected = preconditions::optional_revision(&headers)?;

    Ok(Json(
        state
            .service
            .change_catalogue(&operator, command, expected.as_ref())
            .await?,
    ))
}
