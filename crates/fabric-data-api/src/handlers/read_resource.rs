//! `GET /data/{resource}/{key}`

use axum::extract::{Path, State};
use axum::Json;
use fabric_identity::TenantIdentity;

use crate::handlers::parse_resource;
use crate::{DataApiError, DataApiState, RowResponse};

/// Reads one record by key.
///
/// A key belonging to another tenant returns 404, not 403. The lookup is scoped
/// to this tenant before it runs, so the record genuinely does not exist as far
/// as this request is concerned — and answering 403 would confirm that the key
/// exists somewhere, which is a cross-tenant information leak dressed up as
/// helpfulness.
#[tracing::instrument(skip_all, fields(tenant_id = %identity.tenant(), logical_resource = %resource))]
pub(crate) async fn read_resource(
    State(state): State<DataApiState>,
    identity: TenantIdentity,
    Path((resource, key)): Path<(String, String)>,
) -> Result<Json<RowResponse>, DataApiError> {
    let resource = parse_resource(&resource)?;

    let record = state.service.read(&identity, &resource, &key).await?;

    Ok(Json(record))
}
