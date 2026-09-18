//! Every route under `/platform`.
//!
//! Pulled out of `routes.rs` so that file stays the index it means to be
//! rather than the place every route's `.route(...)` call actually lives —
//! see `routes/integrations.rs` for the same move made for the two Git
//! integrations. The table in `routes.rs` still names every path below;
//! only the building of them is here.

use axum::routing::{get, post, put};
use axum::Router;

use crate::handlers;
use crate::state::ControlPlaneState;

/// Builds every `/platform` route.
pub(super) fn routes() -> Router<ControlPlaneState> {
    Router::new()
        .route("/platform", get(handlers::get_platform))
        // The component *is* named, and the environment still is not: a
        // component name is a key looked up in a manifest this platform
        // already trusts, unlike the environment parameter that used to be here.
        .route(
            "/platform/components/{component}/hold",
            put(handlers::pause_component).delete(handlers::resume_component),
        )
        .route(
            "/platform/components/{component}/versions",
            get(handlers::rollback_candidates),
        )
        // A POST, because it is an act rather than a resource an operator
        // composed: they name a version, and what gets written — three digests
        // and a hold, in one commit — is the platform's to resolve.
        .route(
            "/platform/components/{component}/rollback",
            post(handlers::roll_back_component),
        )
        .route("/platform/data-sources", get(handlers::list_data_sources))
        .route(
            "/platform/data-sources/{data_source_id}",
            put(handlers::declare_data_source).delete(handlers::remove_data_source),
        )
}
