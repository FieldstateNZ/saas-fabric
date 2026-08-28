//! The Data API's HTTP surface.

use axum::middleware;
use axum::routing::get;
use axum::Router;

use crate::handlers;
use crate::{request_id, DataApiState};

/// The full external path prefix every route in this crate is nested under.
///
/// `/v1` versions the contract; `/data` names the domain within it. Building
/// the whole prefix here — rather than this crate owning `/v1` and the host
/// nesting at `/data`, which would produce `/data/v1/...` — keeps the mapping
/// from a version to the path segment that carries it in one file, and
/// yields the more conventional `/v1/data/...` shape. See docs/README.md's
/// "Versioning" section for the policy this constant exists to enforce: `v1`
/// is additive-only, and a breaking change ships as `/v2` mounted alongside
/// it, never replacing it in place. `pub` so the host and this crate's own
/// tests share one source of truth rather than duplicating the literal.
pub const API_PREFIX: &str = "/v1/data";

/// Builds the router for the Data API's full external path, `/v1/data`.
///
/// The host mounts this router as-is — it must not nest it under a further
/// prefix of its own, since the version is already part of the path this
/// function builds. Every path this crate serves is visible here, in one
/// file. Note what is *not* in any of them: a tenant. No path segment, no
/// query parameter, and no header selects a tenant — it comes from the
/// bearer token (§11), so the same URL means different data for different
/// callers, which is the entire point.
///
/// ```text
/// GET    /v1/data/{resource}          list
/// POST   /v1/data/{resource}          create
/// GET    /v1/data/{resource}/{key}    read
/// PATCH  /v1/data/{resource}/{key}    update
/// DELETE /v1/data/{resource}/{key}    delete
/// ```
///
/// `PATCH` rather than `PUT`: the update handler applies the fields it is given
/// and leaves the rest alone, which is a patch. Offering `PUT` would imply
/// whole-record replacement, and a caller omitting a field would silently null
/// it.
pub fn data_routes(state: DataApiState) -> Router {
    let resources = Router::new()
        .route(
            "/{resource}",
            get(handlers::list_resource).post(handlers::create_resource),
        )
        .route(
            "/{resource}/{key}",
            get(handlers::read_resource)
                .patch(handlers::update_resource)
                .delete(handlers::delete_resource),
        );

    Router::new()
        .nest(API_PREFIX, resources)
        // Applied at the router, not per handler, so no response — success or
        // failure — can leave without a correlation id (§29, item 57).
        .layer(middleware::from_fn(request_id::middleware))
        .with_state(state)
}
