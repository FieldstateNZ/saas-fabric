//! The image registries' routes (ADR 0026 section 5).
//!
//! ```text
//! GET/POST   /api/integrations/registries                         list / register one           (201)
//! DELETE     /api/integrations/registries/{host}                  remove one                    (204)
//! PUT/DELETE /api/integrations/registries/{host}/credential       set or replace / remove its credential
//! PUT/DELETE /api/integrations/registries/{host}/repositories/entry/{*path}  register / remove {host}/{path}
//! GET        /api/integrations/registries/{host}/versions/{*path} {host}/{path}'s version tags, newest first
//! ```
//!
//! A fixed path, not `/api/integrations/{kind}`: `registries` is a route
//! family, and `{host}` is a key into records this platform holds. The two
//! repository routes sit under `entry/` because a wildcard must end a route —
//! the rule the secret routes follow for the same reason.

use axum::routing::{get, put};
use axum::Router;

use crate::handlers;
use crate::state::ControlPlaneState;

/// Every registry route, each behind the `Operator` extractor.
pub(super) fn routes() -> Router<ControlPlaneState> {
    Router::new()
        .route(
            "/integrations/registries",
            get(handlers::list_registries).post(handlers::register_registry),
        )
        .route(
            "/integrations/registries/{host}",
            axum::routing::delete(handlers::remove_registry),
        )
        .route(
            "/integrations/registries/{host}/credential",
            put(handlers::set_registry_credential).delete(handlers::remove_registry_credential),
        )
        .route(
            "/integrations/registries/{host}/repositories/entry/{*path}",
            put(handlers::add_registry_repository).delete(handlers::remove_registry_repository),
        )
        .route(
            "/integrations/registries/{host}/versions/{*path}",
            get(handlers::registry_versions),
        )
}
