//! `POST /data/{resource}`

use axum::extract::{Path, State};
use axum::Json;
use fabric_identity::TenantIdentity;
use http::StatusCode;
use serde_json::{Map, Value};

use crate::extraction::BoundedJson;
use crate::handlers::parse_resource;
use crate::{DataApiError, DataApiState, WriteResponse};

/// Creates one or more records.
///
/// Accepts either a single object or an array of them, because both are natural
/// things for a client to send and rejecting one would be pedantry.
///
/// The caller does not supply a tenant, and cannot: any tenant discriminator in
/// the payload is overwritten downstream by
/// [`MutationSpec::for_target`](fabric_connector::MutationSpec::for_target). A
/// caller cannot create a record belonging to another tenant.
#[tracing::instrument(skip_all, fields(tenant_id = %identity.tenant(), logical_resource = %resource))]
pub(crate) async fn create_resource(
    State(state): State<DataApiState>,
    identity: TenantIdentity,
    Path(resource): Path<String>,
    BoundedJson(body): BoundedJson<Value>,
) -> Result<(StatusCode, Json<WriteResponse>), DataApiError> {
    let resource = parse_resource(&resource)?;
    let rows = to_rows(body)?;

    let response = state.service.create(&identity, &resource, rows).await?;

    Ok((StatusCode::CREATED, Json(response)))
}

/// Normalises a single object or an array of objects into a list of records.
fn to_rows(body: Value) -> Result<Vec<Map<String, Value>>, DataApiError> {
    match body {
        Value::Object(object) => Ok(vec![object]),

        Value::Array(items) => items
            .into_iter()
            .map(|item| match item {
                Value::Object(object) => Ok(object),
                _ => Err(DataApiError::BadRequest(
                    "every element must be an object".to_owned(),
                )),
            })
            .collect(),

        _ => Err(DataApiError::BadRequest(
            "the request body must be an object or an array of objects".to_owned(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_single_object_becomes_one_record() {
        let body = serde_json::json!({"name": "Alice"});

        assert_eq!(to_rows(body).unwrap().len(), 1);
    }

    #[test]
    fn an_array_becomes_several_records() {
        let body = serde_json::json!([{"name": "Alice"}, {"name": "Bob"}]);

        assert_eq!(to_rows(body).unwrap().len(), 2);
    }

    #[test]
    fn a_scalar_body_is_rejected() {
        assert!(to_rows(serde_json::json!("Alice")).is_err());
    }

    #[test]
    fn an_array_containing_a_non_object_is_rejected() {
        let body = serde_json::json!([{"name": "Alice"}, 42]);

        assert!(to_rows(body).is_err());
    }
}
