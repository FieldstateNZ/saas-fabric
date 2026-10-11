//! A client's lifecycle writes (D01-14).
//!
//! ```text
//! POST /api/clients/{clientId}/tombstone              retire it, retaining its state   (If-Match)
//! POST /api/clients/{clientId}/deletion-confirmation  confirm deletion; records intent  (If-Match)
//! ```
//!
//! Two routes, not one with a mode: confirming deletion is a separate act at
//! the revision the tombstone produced. There is no route back from either.

use axum::routing::post;
use axum::Router;

use crate::handlers;
use crate::state::ControlPlaneState;

/// Both lifecycle routes, each behind the `Operator` extractor.
pub(super) fn routes() -> Router<ControlPlaneState> {
    Router::new()
        .route("/clients/{client_id}/tombstone", post(handlers::tombstone_client))
        .route(
            "/clients/{client_id}/deletion-confirmation",
            post(handlers::confirm_client_deletion),
        )
}
