//! A client's lifecycle writes (D01-14).
//!
//! ```text
//! POST /api/clients/{clientId}/tombstone              retire it, retaining its state   (If-Match)
//! POST /api/clients/{clientId}/deletion-confirmation  confirm deletion; records intent  (If-Match)
//! POST /api/clients/{clientId}/restoration            reverse a tombstone               (If-Match)
//! ```
//!
//! Three routes, not one with a mode: confirming deletion is a separate act
//! at the revision the tombstone produced, and restoring is the explicit
//! write that reverses a tombstone. There is no route back from a confirmed
//! deletion.

use axum::routing::post;
use axum::Router;

use crate::handlers;
use crate::state::ControlPlaneState;

/// The lifecycle routes, each behind the `Operator` extractor.
pub(super) fn routes() -> Router<ControlPlaneState> {
    Router::new()
        .route("/clients/{client_id}/tombstone", post(handlers::tombstone_client))
        .route(
            "/clients/{client_id}/deletion-confirmation",
            post(handlers::confirm_client_deletion),
        )
        .route("/clients/{client_id}/restoration", post(handlers::restore_client))
}
