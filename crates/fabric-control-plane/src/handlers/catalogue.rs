//! Operator-authenticated catalogue and client-product routes.
use crate::{
    extraction::{BoundedJson, ClientPath},
    preconditions,
    state::ControlPlaneState,
    ControlPlaneError, Operator, StoredClient,
};
use axum::{extract::State, Json};
use fabric_client_model::catalogue::{
    CatalogueCommand, ClientProductRequest, CreateClientRequest, StoredCatalogue,
};
use http::{HeaderMap, StatusCode};

pub(crate) async fn get_catalogue(
    _operator: Operator,
    State(state): State<ControlPlaneState>,
) -> Result<Json<StoredCatalogue>, ControlPlaneError> {
    Ok(Json(state.service.catalogue().await?))
}
pub(crate) async fn change_catalogue(
    operator: Operator,
    State(state): State<ControlPlaneState>,
    headers: HeaderMap,
    BoundedJson(command): BoundedJson<CatalogueCommand>,
) -> Result<Json<StoredCatalogue>, ControlPlaneError> {
    let expected = if headers.get(http::header::IF_NONE_MATCH).is_some_and(|v| v == "*")
        && !headers.contains_key(http::header::IF_MATCH)
    {
        None
    } else {
        Some(preconditions::required_revision(&headers)?)
    };
    Ok(Json(
        state
            .service
            .change_catalogue(&operator, command, expected.as_ref())
            .await?,
    ))
}
pub(crate) async fn create_client(
    operator: Operator,
    State(state): State<ControlPlaneState>,
    BoundedJson(request): BoundedJson<CreateClientRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), ControlPlaneError> {
    let stored = state.service.create_client(&operator, request).await?;
    let body = product_response(&state, &stored)?;
    crate::converge::in_background(&state, &operator);
    Ok((StatusCode::CREATED, Json(body)))
}
pub(crate) async fn get_product(
    operator: Operator,
    State(state): State<ControlPlaneState>,
    ClientPath(id): ClientPath,
) -> Result<Json<serde_json::Value>, ControlPlaneError> {
    let _ = operator;
    let stored = state.service.get(&id).await?;
    Ok(Json(product_response(&state, &stored)?))
}
pub(crate) async fn put_product(
    operator: Operator,
    State(state): State<ControlPlaneState>,
    ClientPath(id): ClientPath,
    headers: HeaderMap,
    BoundedJson(request): BoundedJson<ClientProductRequest>,
) -> Result<Json<serde_json::Value>, ControlPlaneError> {
    let expected = preconditions::required_revision(&headers)?;
    let stored = state
        .service
        .set_product(&operator, &id, request, &expected)
        .await?;
    let body = product_response(&state, &stored)?;
    crate::converge::in_background(&state, &operator);
    Ok(Json(body))
}
fn product_response(
    state: &ControlPlaneState,
    stored: &StoredClient,
) -> Result<serde_json::Value, ControlPlaneError> {
    let product = stored
        .document
        .product()
        .map_err(ControlPlaneError::InvalidRequest)?;
    let client = crate::models::ClientResponse::from_stored(stored);
    let applications = product.applications.iter().map(|assignment| serde_json::json!({ "applicationId": assignment.application_id, "components": assignment.components(), "navigation": assignment.navigation() })).collect::<Vec<_>>();
    Ok(
        serde_json::json!({ "client": client, "product": product, "resolved": applications, "reconciliation": state.service.reconciliation(stored) }),
    )
}
/// Shared projection used by the activity listing.
pub(crate) async fn activity(
    _operator: Operator,
    State(state): State<ControlPlaneState>,
) -> Result<Json<serde_json::Value>, ControlPlaneError> {
    let mut actions = state.service.catalogue().await?.catalogue.activity;
    for stored in state.service.list().await? {
        actions.extend(
            stored
                .document
                .product()
                .map_err(ControlPlaneError::InvalidRequest)?
                .activity,
        );
    }
    actions.sort_by_key(|action| std::cmp::Reverse(action.at));
    Ok(Json(serde_json::json!({ "activity": actions })))
}
pub(crate) async fn operator_profile(operator: Operator) -> Json<serde_json::Value> {
    Json(serde_json::json!({ "subject": operator.subject() }))
}
