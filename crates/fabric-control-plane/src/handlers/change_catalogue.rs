//! `POST /api/catalogue`

use axum::extract::State;
use axum::Json;
use fabric_client_model::catalogue::{CatalogueCommand, StoredCatalogue};
use http::HeaderMap;

use super::select_component::{select_component, Selection};
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
///
/// Selecting a component version takes a path of its own: the server
/// resolves it against the registries before the catalogue's pure half
/// writes it (ADR 0026 section 7). Every other command is applied as it
/// always was.
pub(crate) async fn change_catalogue(
    operator: Operator,
    State(state): State<ControlPlaneState>,
    headers: HeaderMap,
    BoundedJson(command): BoundedJson<CatalogueCommand>,
) -> Result<Json<StoredCatalogue>, ControlPlaneError> {
    let expected = preconditions::optional_revision(&headers)?;

    let stored = match command {
        CatalogueCommand::SelectComponentVersion {
            id,
            component,
            repository,
            version,
        } => {
            let selection = Selection {
                application: id,
                component,
                repository,
                version,
            };
            select_component(&state, &operator, &selection, expected.as_ref()).await?
        }
        command @ (CatalogueCommand::CreateApplication { .. }
        | CatalogueCommand::SaveApplication { .. }
        | CatalogueCommand::PublishApplication { .. }
        | CatalogueCommand::SaveDefinition { .. }
        | CatalogueCommand::SaveSettings { .. }
        | CatalogueCommand::SaveEnvironment { .. }) => {
            state
                .service
                .change_catalogue(&operator, command, expected.as_ref())
                .await?
        }
    };
    Ok(Json(stored))
}
