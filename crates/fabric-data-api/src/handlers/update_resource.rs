//! `PATCH /data/{resource}/{key}`

use axum::extract::{Path, State};
use axum::Json;
use fabric_identity::TenantIdentity;
use serde_json::{Map, Value};

use crate::handlers::parse_resource;
use crate::{DataApiError, DataApiState, WriteResponse};

/// Updates the given fields of one record.
///
/// A patch, not a replacement: fields absent from the body are left alone.
/// Whole-record replacement would mean a client that omits a field silently
/// nulls it, which is a bad default for an API whose callers may be working
/// from a partial view of the record.
#[tracing::instrument(skip_all, fields(tenant_id = %identity.tenant(), logical_resource = %resource))]
pub(crate) async fn update_resource(
    State(state): State<DataApiState>,
    identity: TenantIdentity,
    Path((resource, key)): Path<(String, String)>,
    Json(body): Json<Value>,
) -> Result<Json<WriteResponse>, DataApiError> {
    let resource = parse_resource(&resource)?;
    let changes = to_changes(body)?;

    let response = state.service.update(&identity, &resource, &key, &changes).await?;

    Ok(Json(response))
}

/// Requires the body to be a JSON object.
fn to_changes(body: Value) -> Result<Map<String, Value>, DataApiError> {
    match body {
        Value::Object(object) => Ok(object),
        _ => Err(DataApiError::BadRequest(
            "the request body must be an object".to_owned(),
        )),
    }
}
