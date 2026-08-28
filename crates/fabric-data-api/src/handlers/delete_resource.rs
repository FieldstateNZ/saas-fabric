//! `DELETE /data/{resource}/{key}`

use axum::extract::{Path, State};
use axum::Json;
use fabric_identity::TenantIdentity;

use crate::handlers::parse_resource;
use crate::{DataApiError, DataApiState, WriteResponse};

/// Deletes one record by key.
///
/// There is deliberately no unfiltered delete anywhere in this API. The route
/// requires a key, so a caller cannot ask to empty a collection whatever their
/// scopes — and under discriminator isolation, an unscoped delete would reach
/// every tenant's rows in a shared table.
///
/// Deleting a key that belongs to another tenant reports zero rows affected,
/// because the scoped predicate matches nothing. It does not report an error,
/// for the same reason a read returns 404: distinguishing "not yours" from
/// "does not exist" tells the caller something about other tenants.
#[tracing::instrument(skip_all, fields(tenant_id = %identity.tenant(), logical_resource = %resource))]
pub(crate) async fn delete_resource(
    State(state): State<DataApiState>,
    identity: TenantIdentity,
    Path((resource, key)): Path<(String, String)>,
) -> Result<Json<WriteResponse>, DataApiError> {
    let resource = parse_resource(&resource)?;

    let response = state.service.delete(&identity, &resource, &key).await?;

    Ok(Json(response))
}
