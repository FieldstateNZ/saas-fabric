//! A client's identity.
//!
//! ```text
//! GET/PUT /api/clients/{clientId}/identity              its identity and reconciliation state / replace it   (If-Match)
//! POST    /api/clients/{clientId}/identity/reassertion  overwrite unsupported hand edits in the provider   (If-Match)
//! ```
//!
//! The re-assertion is the only route in this API that reaches the identity
//! provider for one client, and it does so only as the operator asking
//! (ADR 0012, D01-19b). See `handlers::reassert_identity`.

use axum::routing::{get, post};
use axum::Router;

use crate::handlers;
use crate::state::ControlPlaneState;

/// Both identity routes, each behind the `Operator` extractor.
pub(super) fn routes() -> Router<ControlPlaneState> {
    Router::new()
        .route(
            "/clients/{client_id}/identity",
            get(handlers::get_identity).put(handlers::put_identity),
        )
        .route(
            "/clients/{client_id}/identity/reassertion",
            post(handlers::reassert_identity),
        )
}
