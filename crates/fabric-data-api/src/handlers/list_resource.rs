//! `GET /data/{resource}`

use axum::extract::{Path, RawQuery, State};
use axum::Json;
use fabric_identity::TenantIdentity;

use crate::handlers::parse_resource;
use crate::{DataApiError, DataApiState, ListQuery, ListResponse};

/// Lists records of a logical resource.
///
/// The raw query string is taken rather than a typed struct because most of it
/// is dynamic: `limit`, `offset`, `sort`, and `select` are reserved, and every
/// other parameter is an equality filter on a field. A `Deserialize` struct
/// cannot express "and any other key is a field name".
#[tracing::instrument(skip_all, fields(tenant_id = %identity.tenant(), logical_resource = %resource))]
pub(crate) async fn list_resource(
    State(state): State<DataApiState>,
    identity: TenantIdentity,
    Path(resource): Path<String>,
    RawQuery(raw_query): RawQuery,
) -> Result<Json<ListResponse>, DataApiError> {
    let resource = parse_resource(&resource)?;
    let definition = state.service.catalog().resolve(&resource)?;
    let query = ListQuery::parse(raw_query.as_deref().unwrap_or_default(), definition)?;

    let response = state.service.list(&identity, &resource, &query).await?;

    Ok(Json(response))
}
