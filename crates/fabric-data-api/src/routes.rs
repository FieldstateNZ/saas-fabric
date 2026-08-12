//! The Data API's HTTP surface.

use axum::routing::get;
use axum::Router;

use crate::handlers;
use crate::DataApiState;

/// Builds the router for `/data`.
///
/// Every path this crate serves is visible here, in one file. Note what is
/// *not* in any of them: a tenant. No path segment, no query parameter, and no
/// header selects a tenant — it comes from the bearer token (§11), so the same
/// URL means different data for different callers, which is the entire point.
///
/// ```text
/// GET    /data/{resource}          list
/// POST   /data/{resource}          create
/// GET    /data/{resource}/{key}    read
/// PATCH  /data/{resource}/{key}    update
/// DELETE /data/{resource}/{key}    delete
/// ```
///
/// `PATCH` rather than `PUT`: the update handler applies the fields it is given
/// and leaves the rest alone, which is a patch. Offering `PUT` would imply
/// whole-record replacement, and a caller omitting a field would silently null
/// it.
pub fn data_routes(state: DataApiState) -> Router {
    Router::new()
        .route(
            "/{resource}",
            get(handlers::list_resource).post(handlers::create_resource),
        )
        .route(
            "/{resource}/{key}",
            get(handlers::read_resource)
                .patch(handlers::update_resource)
                .delete(handlers::delete_resource),
        )
        .with_state(state)
}
