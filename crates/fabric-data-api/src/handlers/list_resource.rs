//! `GET /data/{resource}`

use axum::extract::{Path, RawQuery, State};
use axum::Json;
use fabric_identity::TenantIdentity;

use crate::handlers::parse_resource;
use crate::{DataApiError, DataApiState, ListResponse};

/// Lists records of a logical resource.
///
/// The raw query string is taken rather than a typed struct because most of it
/// is dynamic: `limit`, `offset`, `sort`, and `select` are reserved, and every
/// other parameter is an equality filter on a field. A `Deserialize` struct
/// cannot express "and any other key is a field name".
///
/// It is also passed on **unparsed**. Parsing checks field names against the
/// resource's `queryable_fields`, so a parse failure distinguishes a real field
/// from an invented one — a fact about the resource that a caller who is about
/// to be refused must not be able to read off a status code. The parse belongs
/// behind authorization, in [`DataApiService::list`](crate::DataApiService),
/// and this handler could not do it early even if someone tried: nothing on the
/// service hands out a `ResourceDefinition`, so there is nothing to parse
/// against until `prepare` has produced one.
#[tracing::instrument(skip_all, fields(tenant_id = %identity.tenant(), logical_resource = %resource))]
pub(crate) async fn list_resource(
    State(state): State<DataApiState>,
    identity: TenantIdentity,
    Path(resource): Path<String>,
    RawQuery(raw_query): RawQuery,
) -> Result<Json<ListResponse>, DataApiError> {
    let resource = parse_resource(&resource)?;

    let response = state
        .service
        .list(&identity, &resource, raw_query.as_deref().unwrap_or_default())
        .await?;

    Ok(Json(response))
}
