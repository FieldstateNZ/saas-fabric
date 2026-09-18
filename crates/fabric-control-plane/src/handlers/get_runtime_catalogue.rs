//! `GET /api/catalogue/runtime`: the catalogue the runtime would be given.

use axum::extract::State;
use axum::Json;
use fabric_client_model::catalogue::{CatalogueConflict, DerivedResource};
use fabric_client_model::{ClientRevision, DesiredStateError};
use fabric_core::OperationKind;

use crate::state::ControlPlaneState;
use crate::{ControlPlaneError, Operator};

/// The derived runtime catalogue, as the console shows it.
///
/// Derived fresh from the product catalogue on every read rather than
/// stored anywhere (ADR 0023 part 3), so what this answers is exactly what
/// publishing now would hand the runtime. `revision` is the product
/// catalogue's, because that is the document a change to this view would be
/// made in.
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RuntimeCatalogueBody {
    /// The product catalogue's revision; `null` before its first write.
    revision: Option<ClientRevision>,

    /// Every resource the runtime would be given, sorted by name.
    resources: Vec<RuntimeResourceRow>,
}

/// One derived resource: its definition, and where it came from.
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RuntimeResourceRow {
    /// The name callers address the resource by.
    name: String,

    /// The application whose newest release declares it.
    application: String,

    /// That release's version.
    version: u32,

    /// The logical data source the resource lives in, never a physical id.
    data_source: String,

    /// The collection the connector knows it by.
    collection: String,

    /// The field identifying one row.
    key_field: String,

    /// What callers may do with it.
    operations: Vec<OperationKind>,

    /// Which fields callers may filter, sort and project on; empty means
    /// unrestricted.
    queryable_fields: Vec<String>,
}

impl RuntimeResourceRow {
    fn of(resource: DerivedResource) -> Self {
        let DerivedResource {
            name,
            definition,
            application,
            version,
        } = resource;

        Self {
            name: name.to_string(),
            application: application.to_string(),
            version,
            data_source: definition.data_source.to_string(),
            collection: definition.collection.to_string(),
            key_field: definition.key_field.to_string(),
            operations: definition.operations,
            queryable_fields: definition
                .queryable_fields
                .into_iter()
                .map(|field| field.to_string())
                .collect(),
        }
    }
}

/// `GET /api/catalogue/runtime`.
///
/// # Errors
///
/// [`ControlPlaneError::InvalidCatalogue`] — `500 desired_state_invalid` —
/// when two applications' newest releases declare one resource name. The
/// console's own publish refuses that before it can be stored, so reaching
/// it means the catalogue was edited by hand; the message names the
/// resource and both applications, and no retry fixes it.
pub(crate) async fn get_runtime_catalogue(
    _operator: Operator,
    State(state): State<ControlPlaneState>,
) -> Result<Json<RuntimeCatalogueBody>, ControlPlaneError> {
    let stored = state.service.catalogue().await?;

    let derived = stored
        .catalogue
        .runtime_catalogue()
        .map_err(
            |conflict: CatalogueConflict| ControlPlaneError::InvalidCatalogue {
                source: DesiredStateError::CatalogueMalformed {
                    detail: conflict.to_string(),
                },
            },
        )?;

    Ok(Json(RuntimeCatalogueBody {
        revision: stored.revision,
        resources: derived
            .resources
            .into_iter()
            .map(RuntimeResourceRow::of)
            .collect(),
    }))
}
